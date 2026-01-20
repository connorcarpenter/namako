//! `#[given]`, `#[when]` and `#[then]` attribute macros implementation.

use std::{iter, mem};

use cucumber_expressions::{Expression, Parameter, SingleExpression, Spanned};
use inflections::case::to_pascal_case;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse::{Parse, ParseStream}, parse_quote, spanned::Spanned as _, Signature};

/// Names of default [`Parameter`]s.
const DEFAULT_PARAMETERS: [&str; 5] = ["int", "float", "word", "string", ""];

/// Generates code of `#[given]`, `#[when]` and `#[then]` attribute macros
/// expansion.
pub(crate) fn step(
    attr_name: &'static str,
    args: TokenStream,
    input: TokenStream,
) -> syn::Result<TokenStream> {
    Step::parse(attr_name, args, input).and_then(Step::expand)
}

/// Parsed state (ready for code generation) of the attribute and the function
/// it's applied to.
#[derive(Clone, Debug)]
struct Step {
    /// Name of the attribute (`given`, `when` or `then`).
    attr_name: &'static str,

    /// Argument of the attribute.
    attr_arg: AttributeArgument,

    /// Function the attribute is applied to.
    func: syn::ItemFn,

    /// Name of the function argument representing a [`gherkin::Step`]
    /// reference.
    ///
    /// [`gherkin::Step`]: https://bit.ly/3j42hcd
    arg_name_of_step_context: Option<syn::Ident>,
}

impl Step {
    /// Parses [`Step`] definition from the attribute macro input.
    fn parse(
        attr_name: &'static str,
        attr: TokenStream,
        body: TokenStream,
    ) -> syn::Result<Self> {
        let attr_arg = syn::parse2::<AttributeArgument>(attr)?;
        let mut func = syn::parse2::<syn::ItemFn>(body)?;

        let step_arg_name = {
            let (arg_marked_as_step, _) =
                remove_all_attrs_if_needed("step", &mut func);

            match arg_marked_as_step.len() {
                0 => Ok(None),
                1 => {
                    let (ident, _) = parse_fn_arg(arg_marked_as_step[0])?;
                    Ok(Some(ident.clone()))
                }
                _ => Err(syn::Error::new(
                    arg_marked_as_step[1].span(),
                    "only 1 step argument is allowed",
                )),
            }
        }?
        .or_else(|| {
            func.sig.inputs.iter().find_map(|arg| {
                if let Ok((ident, _)) = parse_fn_arg(arg)
                    && ident == "step"
                {
                    return Some(ident.clone());
                }
                None
            })
        });

        Ok(Self {
            attr_name,
            attr_arg,
            func,
            arg_name_of_step_context: step_arg_name,
        })
    }

    /// Expands generated code of this [`Step`] definition.
    fn expand(mut self) -> syn::Result<TokenStream> {
        // Parse the context type and determine if it needs lifetime injection.
        // Two patterns are supported:
        // 1. Reference types: `&mut World` or `&World` - blanket impl handles these
        // 2. Wrapper types: `TestWorldMut` - needs lifetime injection to become `TestWorldMut<'__ctx>`
        let (ctx_type, needs_lifetime) = parse_context_type_and_mode(&self.func.sig, self.attr_name)?;

        // Rewrite the function signature to inject the lifetime (only for wrapper types)
        rewrite_signature_with_lifetime(&mut self.func, needs_lifetime)?;

        let func = &self.func;
        let func_name = &func.sig.ident;
        let step_type = self.step_type();
        let (func_args, addon_parsing) =
            self.fn_arguments_and_additional_parsing()?;

        let regex = self.gen_regex()?;
        let allow_trivial_regex_attr = quote! {};

        let awaiting = func.sig.asyncness.map(|_| quote! { .await });

        // For Then steps returning AssertOutcome, don't apply unwrapping
        // For other non-unit returns (Result types), apply unwrapping
        let returns_assert_outcome = self.returns_assert_outcome();
        let unwrapping = if returns_assert_outcome {
            None
        } else {
            (!self.returns_unit()).then(|| quote! { .unwrap_or_else(|e| panic!("{}", e)) })
        };

        // NPAP v1: Compute binding_id and impl_hash at compile time
        let kind = inflections::case::to_pascal_case(self.attr_name);
        let expression = match &self.attr_arg {
            AttributeArgument::Expression(lit) => lit.value(),
        };
        let binding_id = crate::npap::generate_binding_id(&kind, &expression);
        let impl_hash = crate::npap::generate_impl_hash(&func.block);

        // NPAP v1: Extract signature information
        let signature_info = self.extract_signature_info()?;
        let captures_arity = signature_info.captures_arity;
        let accepts_docstring = signature_info.accepts_docstring;
        let accepts_datatable = signature_info.accepts_datatable;

        // Context-first ABI: construct ctx from world and pass to user fn
        // Given/When: use ctx_mut() for mutable context
        // Then: uses polling loop with ExpectCtx wrapping (handled separately below)
        let ctx_construction = if self.attr_name == "then" {
            // For Then steps, ctx_construction is handled inside the polling loop
            quote! {}
        } else {
            quote! {
                let mut __namako_ctx_arg = ::namako::World::ctx_mut(__namako_world);
            }
        };

        // Pass context as first arg to user function
        // Respects whether the user function asks for &mut Ctx, &Ctx, or Ctx (value)
        let first_arg = func.sig.inputs.first().expect("step function must have at least one argument");
        let (is_reference, is_mutable) = if let syn::FnArg::Typed(pat_type) = first_arg {
            if let syn::Type::Reference(r) = pat_type.ty.as_ref() {
                (true, r.mutability.is_some())
            } else {
                (false, false)
            }
        } else {
            (false, false)
        };

        let ctx_arg = if self.attr_name == "then" {
            // For Then steps, the closure arg `__namako_ctx_arg` is ALREADY `&Ctx`.
            if is_reference {
                 quote! { __namako_ctx_arg }
            } else {
                 quote! { *__namako_ctx_arg }
            }
        } else {
            // For Given/When, `__namako_ctx_arg` is `Ctx` (value).
            if is_mutable {
                quote! { &mut __namako_ctx_arg }
            } else if is_reference {
                quote! { &__namako_ctx_arg }
            } else {
                quote! { __namako_ctx_arg }
            }
        };

        // Generate different func body for Then vs Given/When
        let func_body = if self.attr_name == "then" {
            // Then steps: delegate to World's assert_then() method
            // The World controls execution semantics (single-shot vs polling)
            if returns_assert_outcome {
                // User function returns AssertOutcome directly - use it as-is
                // Still wrap in catch_unwind to handle any panics gracefully
                quote! {
                    func: |__namako_world, __namako_ctx| {
                        ::std::boxed::Box::pin(async move {
                            #addon_parsing

                            <WorldAlias as ::namako::World>::assert_then(
                                __namako_world,
                                |__namako_ctx_arg| {
                                    let result = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                                        #func_name(#ctx_arg, #func_args)
                                            #awaiting
                                    }));

                                    match result {
                                        Ok(outcome) => outcome,
                                        Err(payload) => {
                                            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                                                s.to_string()
                                            } else if let Some(s) = payload.downcast_ref::<String>() {
                                                s.clone()
                                            } else {
                                                "Unknown panic payload".to_string()
                                            };
                                            ::namako::codegen::AssertOutcome::Failed(msg)
                                        }
                                    }
                                }
                            );
                        })
                    },
                }
            } else {
                // User function returns () - wrap with panic catching
                quote! {
                    func: |__namako_world, __namako_ctx| {
                        ::std::boxed::Box::pin(async move {
                            #addon_parsing

                            <WorldAlias as ::namako::World>::assert_then(
                                __namako_world,
                                |__namako_ctx_arg| {
                                    let result = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                                        let _ = #func_name(#ctx_arg, #func_args)
                                            #awaiting
                                            #unwrapping;
                                    }));

                                    match result {
                                        Ok(_) => ::namako::codegen::AssertOutcome::Passed(()),
                                        Err(payload) => {
                                            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                                                s.to_string()
                                            } else if let Some(s) = payload.downcast_ref::<String>() {
                                                s.clone()
                                            } else {
                                                "Unknown panic payload".to_string()
                                            };
                                            ::namako::codegen::AssertOutcome::Failed(msg)
                                        }
                                    }
                                }
                            );
                        })
                    },
                }
            }
        } else {
            // Given/When steps: simple context construction and call
            quote! {
                func: |__namako_world, __namako_ctx| {
                    let f = async move {
                        #addon_parsing
                        #ctx_construction
                        let _ = #func_name(#ctx_arg, #func_args)
                            #awaiting
                            #unwrapping;
                    };
                    ::std::boxed::Box::pin(f)
                },
            }
        };

        Ok(quote! {
            #func

            #[automatically_derived]
            ::namako::codegen::submit!({
                // Derive World type from context type via StepContext trait
                // Use 'static lifetime for the type alias - we only need the World associated type
                type WorldAlias = <#ctx_type as ::namako::codegen::StepContext>::World;

                // TODO: Remove this, once `#![feature(more_qualified_paths)]`
                //       is stabilized:
                //       https://github.com/rust-lang/rust/issues/86935
                type StepAlias =
                    <WorldAlias as ::namako::codegen::WorldInventory>::#step_type;

                StepAlias {
                    loc: ::namako::step::Location {
                        path: ::std::file!(),
                        line: ::std::line!(),
                        column: ::std::column!(),
                    },
                    // NPAP v1 metadata
                    binding_id: #binding_id,
                    expression: #expression,
                    kind: #kind,
                    impl_hash: #impl_hash,
                    captures_arity: #captures_arity,
                    accepts_docstring: #accepts_docstring,
                    accepts_datatable: #accepts_datatable,
                    // Source symbol: stable identifier per TODO.md §3
                    // Uses module_path!() + function name for AI-friendly navigation
                    source_symbol: ::std::concat!(
                        ::std::module_path!(),
                        "::",
                        ::std::stringify!(#func_name)
                    ),
                    regex: || {
                        #allow_trivial_regex_attr
                        static LAZY: ::std::sync::LazyLock<
                            ::namako::codegen::Regex
                        > = ::std::sync::LazyLock::new(|| { #regex });
                        LAZY.clone()
                    },
                    #func_body
                }
            });
        })
    }

    /// Extracts NPAP v1 signature information from the function.
    ///
    /// Per GOLD_PLAN §4.4:
    /// - `captures_arity`: count of capture parameters (after `&mut World`)
    /// - `accepts_docstring`: true if function has `Option<String>` parameter
    /// - `accepts_datatable`: true if function has `Option<Vec<Vec<String>>>` parameter
    fn extract_signature_info(&self) -> syn::Result<SignatureInfo> {
        let mut captures_arity: u32 = 0;
        let mut accepts_docstring = false;
        let mut accepts_datatable = false;

        // Skip the first argument (&mut World)
        for arg in self.func.sig.inputs.iter().skip(1) {
            // Skip step context argument if present
            if let Some(step_name) = &self.arg_name_of_step_context {
                if let Ok((ident, _)) = parse_fn_arg(arg) {
                    if ident == step_name {
                        continue;
                    }
                }
            }

            if let Ok((_, ty)) = parse_fn_arg(arg) {
                if is_docstring_type(ty) {
                    accepts_docstring = true;
                } else if is_datatable_type(ty) {
                    accepts_datatable = true;
                } else {
                    // It's a capture parameter
                    captures_arity += 1;
                }
            }
        }

        Ok(SignatureInfo {
            captures_arity,
            accepts_docstring,
            accepts_datatable,
        })
    }

    /// Indicates whether this [`Step::func`] return type is `()`.
    fn returns_unit(&self) -> bool {
        match &self.func.sig.output {
            syn::ReturnType::Default => true,
            syn::ReturnType::Type(_, ty) => {
                if let syn::Type::Tuple(syn::TypeTuple { elems, .. }) = &**ty {
                    elems.is_empty()
                } else {
                    false
                }
            }
        }
    }

    /// Indicates whether this [`Step::func`] return type is `AssertOutcome<_>`.
    fn returns_assert_outcome(&self) -> bool {
        match &self.func.sig.output {
            syn::ReturnType::Default => false,
            syn::ReturnType::Type(_, ty) => {
                // Check if it's a path type ending in "AssertOutcome"
                if let syn::Type::Path(type_path) = &**ty {
                    if let Some(segment) = type_path.path.segments.last() {
                        return segment.ident == "AssertOutcome";
                    }
                }
                false
            }
        }
    }

    /// Generates code that prepares function's arguments basing on
    /// [`AttributeArgument`] and additional parsing if it's an
    /// [`AttributeArgument::Regex`].
    fn fn_arguments_and_additional_parsing(
        &self,
    ) -> syn::Result<(TokenStream, Option<TokenStream>)> {
        let is_regex_or_expr = matches!(
            self.attr_arg,
            AttributeArgument::Expression(_),
        );
        let func = &self.func;

        if is_regex_or_expr {
            if let Some(elem_ty) = find_first_slice(&func.sig) {
                let addon_parsing = Some(quote! {
                    let mut __namako_matches = ::std::vec::Vec::with_capacity(
                        __namako_ctx.matches.len().saturating_sub(1),
                    );
                    let mut __namako_iter = __namako_ctx
                        .matches
                        .iter()
                        .skip(1)
                        .enumerate();
                    while let Some((i, (cap_name, s))) =
                        __namako_iter.next()
                    {
                        // Special handling of `cucumber-expressions`
                        // `parameter` with multiple capturing groups.
                        let prefix = cap_name
                            .as_ref()
                            .filter(|n| n.starts_with("__"))
                            .map(|n| {
                                let num_len = n
                                    .chars()
                                    .skip(2)
                                    .take_while(|&c| c != '_')
                                    .map(char::len_utf8)
                                    .sum::<usize>();
                                let len = num_len + b"__".len();
                                n.split_at(len).0
                            });

                        let to_take = __namako_iter
                            .clone()
                            .take_while(|(_, (n, _))| {
                                prefix
                                    .zip(n.as_ref())
                                    .filter(|(prefix, n)| n.starts_with(prefix))
                                    .is_some()
                            })
                            .count();

                        let s = ::std::iter::once(s.as_str())
                            .chain(
                                __namako_iter
                                    .by_ref()
                                    .take(to_take)
                                    .map(|(_, (_, s))| s.as_str()),
                            )
                            .fold(None, |acc, s| {
                                acc.or_else(|| (!s.is_empty()).then_some(s))
                            })
                            .unwrap_or_default();

                        __namako_matches.push(
                            s.parse::<#elem_ty>().unwrap_or_else(|e| panic!(
                                "Failed to parse element at {} '{}': {}",
                                i, s, e,
                            ))
                        );
                    }
                });
                let func_args = func
                    .sig
                    .inputs
                    .iter()
                    .skip(1)
                    .map(|arg| self.borrow_step_or_slice(arg))
                    .collect::<Result<TokenStream, _>>()?;

                Ok((func_args, addon_parsing))
            } else {
                let (idents, parsings): (Vec<_>, Vec<_>) =
                    itertools::process_results(
                        func.sig
                            .inputs
                            .iter()
                            .skip(1)
                            .map(|arg| self.arg_ident_and_parse_code(arg)),
                        |i| i.unzip(),
                    )?;

                let addon_parsing = Some(quote! {
                    let mut __namako_iter = __namako_ctx
                        .matches.iter()
                        .skip(1);
                    #( #parsings )*
                });
                let func_args = quote! {
                    #( #idents, )*
                };

                Ok((func_args, addon_parsing))
            }
        } else if self.arg_name_of_step_context.is_some() {
            Ok((
                quote! { ::std::borrow::Borrow::borrow(&__namako_ctx.step), },
                None,
            ))
        } else {
            Ok((TokenStream::default(), None))
        }
    }

    /// Composes a name of the `namako::codegen::WorldInventory` associated
    /// type to wire this [`Step`] with.
    fn step_type(&self) -> syn::Ident {
        format_ident!("{}", to_pascal_case(self.attr_name))
    }

    /// Returns [`syn::Ident`] and parsing code of the given function's
    /// argument.
    ///
    /// Function's argument type have to implement [`FromStr`].
    ///
    /// [`FromStr`]: std::str::FromStr
    /// [`syn::Ident`]: struct@syn::Ident
    fn arg_ident_and_parse_code<'a>(
        &self,
        arg: &'a syn::FnArg,
    ) -> syn::Result<(&'a syn::Ident, TokenStream)> {
        let (ident, ty) = parse_fn_arg(arg)?;

        let is_ctx_arg =
            self.arg_name_of_step_context.as_ref().is_some_and(|i| i == ident);

        let decl = if is_ctx_arg {
            quote! {
                let #ident =
                    ::std::borrow::Borrow::borrow(&__namako_ctx.step);
            }
        } else {
            let syn::Type::Path(ty) = ty else {
                return Err(syn::Error::new(ty.span(), "type path expected"));
            };

            let not_found_err = format!("{ident} not found");
            let parsing_err = format!(
                "{ident} can not be parsed to {}",
                ty.path
                    .segments
                    .last()
                    .ok_or_else(|| {
                        syn::Error::new(ty.path.span(), "type path expected")
                    })?
                    .ident,
            );

            quote! {
                let #ident = {
                    let (cap_name, s) = __namako_iter
                        .next()
                        .expect(#not_found_err);
                    // Special handling of `cucumber-expressions` `parameter`
                    // with multiple capturing groups.
                    let prefix = cap_name
                        .as_ref()
                        .filter(|n| n.starts_with("__"))
                        .map(|n| {
                            let num_len = n
                                .chars()
                                .skip(2)
                                .take_while(|&c| c != '_')
                                .map(char::len_utf8)
                                .sum::<usize>();
                            let len = num_len + b"__".len();
                            n.split_at(len).0
                        });

                    let to_take = __namako_iter
                        .clone()
                        .take_while(|(n, _)| {
                            prefix.zip(n.as_ref())
                                .filter(|(prefix, n)| n.starts_with(prefix))
                                .is_some()
                        })
                        .count();

                    ::std::iter::once(s.as_str())
                        .chain(
                            __namako_iter
                                .by_ref()
                                .take(to_take)
                                .map(|(_, s)| s.as_str()),
                        )
                        .fold(None, |acc, s| {
                            acc.or_else(|| (!s.is_empty()).then_some(s))
                        })
                        .unwrap_or_default()
                };
                let #ident = #ident.parse::<#ty>().expect(#parsing_err);
            }
        };

        Ok((ident, decl))
    }

    /// Generates code that borrows [`gherkin::Step`] from context if the given
    /// `arg` matches `step_arg_name`, or else borrows parsed slice.
    ///
    /// [`gherkin::Step`]: https://bit.ly/3j42hcd
    fn borrow_step_or_slice(
        &self,
        arg: &syn::FnArg,
    ) -> syn::Result<TokenStream> {
        if let Some(name) = &self.arg_name_of_step_context {
            let (ident, _) = parse_fn_arg(arg)?;
            if name == ident {
                return Ok(quote! {
                    ::std::borrow::Borrow::borrow(&__namako_ctx.step),
                });
            }
        }

        Ok(quote! {
            __namako_matches.as_slice(),
        })
    }

    /// Generates code constructing a [`Regex`] based on an
    /// [`AttributeArgument`].
    ///
    /// # Errors
    ///
    /// - If [`AttributeArgument::Regex`] isn't a valid [`Regex`].
    /// - If [`AttributeArgument::Expression`] passed to
    ///   [`gen_expression_regex()`] errors.
    ///
    /// [`gen_expression_regex()`]: Self::gen_expression_regex
    fn gen_regex(&self) -> syn::Result<TokenStream> {
        let AttributeArgument::Expression(l) = &self.attr_arg;
        self.gen_expression_regex(l)
    }

    /// Generates code constructing [`Regex`] for an
    /// [`AttributeArgument::Expression`].
    ///
    /// # Errors
    ///
    /// If [`Parameters::new()`] errors.
    fn gen_expression_regex(
        &self,
        expr: &syn::LitStr,
    ) -> syn::Result<TokenStream> {
        let expr = expr.value();
        let params = Parameters::new(
            &expr,
            &self.func,
            self.arg_name_of_step_context.as_ref(),
        )?;

        let provider_impl =
            params.gen_provider_impl(&parse_quote! { Provider });
        let const_assertions = params.gen_const_assertions();

        Ok(quote! {{
            #const_assertions

            #[automatically_derived]
            #[derive(Clone, Copy)]
            struct Provider;

            #provider_impl

            // This should never fail because:
            // 1. We checked AST correctness with `Expression::parse()`;
            // 2. Custom `Parameter::REGEX`es are correct due to be validated
            //    in `#[derive(Parameter)]` macro expansion;
            // 3. All the parameter names are equal to the corresponding
            //    function arguments, so we shouldn't see any
            //    `UnknownParameterError`s.
            ::namako::codegen::Expression::regex_with_parameters(
                #expr,
                Provider,
            )
            .unwrap()
        }})
    }
}

/// [`Parameter`] parsed from an [`AttributeArgument::Expression`] along with a
/// [`fn`] argument's [`syn::Type`] corresponding to it.
struct ParameterProvider<'p> {
    /// [`Parameter`] parsed from an [`AttributeArgument::Expression`].
    param: Parameter<Spanned<'p>>,

    /// [`syn::Type`] of the [`fn`] argument corresponding to the [`Parameter`].
    ty: syn::Type,
}

/// Collection of [`ParameterProvider`]s.
struct Parameters<'p>(Vec<ParameterProvider<'p>>);

impl<'p> Parameters<'p> {
    /// Creates new [`Parameters`].
    ///
    /// # Errors
    ///
    /// - If [`Expression::parse()`] errors.
    /// - If [`parse_fn_arg()`] on one of the `func`'s arguments errors.
    /// - If non-default [`Parameter`] doesn't have the corresponding `func`'s
    ///   argument.
    fn new(
        expr: &'p str,
        func: &syn::ItemFn,
        step: Option<&syn::Ident>,
    ) -> syn::Result<Self> {
        let expr = Expression::parse(expr).map_err(|e| {
            syn::Error::new(
                expr.span(),
                format!("invalid Cucumber Expression: {e}"),
            )
        })?;

        let param_tys = func
            .sig
            .inputs
            .iter()
            .skip(1)
            .filter_map(|arg| {
                let (ident, ty) = match parse_fn_arg(arg) {
                    Ok(res) => res,
                    Err(err) => return Some(Err(err)),
                };
                let is_step = step.is_some_and(|s| s == ident);
                (!is_step).then_some(Ok(ty))
            })
            .collect::<syn::Result<Vec<_>>>()?;

        expr.0
            .into_iter()
            .filter_map(|e| match e {
                SingleExpression::Parameter(par) => Some(par),
                SingleExpression::Alternation(_)
                | SingleExpression::Optional(_)
                | SingleExpression::Text(_)
                | SingleExpression::Whitespaces(_) => None,
            })
            .zip(param_tys.into_iter().map(Some).chain(iter::repeat(None)))
            .filter_map(|(ast, param_ty)| {
                if DEFAULT_PARAMETERS.iter().any(|s| s == &**ast) {
                    // If parameter is default, it's OK if there is no type
                    // corresponding to it, as we know its regex.
                    param_ty
                        .cloned()
                        .map(|ty| Ok(ParameterProvider { param: ast, ty }))
                } else if let Some(ty) = param_ty.cloned() {
                    Some(Ok(ParameterProvider { param: ast, ty }))
                } else {
                    Some(Err(syn::Error::new(
                        func.sig.inputs.span(),
                        format!(
                            "function argument corresponding to the `{{{p}}}` \
                             parameter isn't found. Consider adding \
                             argument implementing a `Parameter` trait with \
                             `Parameter::NAME == {p}`.",
                            p = *ast,
                        ),
                    )))
                }
            })
            .collect::<syn::Result<Vec<_>>>()
            .map(Self)
    }

    /// Generates code asserting that all the corresponding
    /// [`ParameterProvider::param`]s and [`ParameterProvider::ty`]s are
    /// correct.
    ///
    /// Here `correct` means one of 2 things:
    /// 1. If a [`ParameterProvider::param`] is one of [`DEFAULT_PARAMETERS`],
    ///    then its [`ParameterProvider::ty`] shouldn't implement a `Parameter`
    ///    trait, Because in case it does, there is a special `Parameter::NAME`,
    ///    that should be used instead of the default one, while it cannot be
    ///    done.
    /// 2. If a [`ParameterProvider::param`] isn't one of
    ///    [`DEFAULT_PARAMETERS`], then its [`ParameterProvider::ty`] must
    ///    implement a `Parameter` trait with
    ///    `Parameter::NAME == `[`ParameterProvider::param`].
    fn gen_const_assertions(&self) -> TokenStream {
        self.0
            .iter()
            .map(|par| {
                let name = par.param.input.fragment();
                let ty = &par.ty;

                if DEFAULT_PARAMETERS.contains(name) {
                    // We do use here custom machinery, rather than using
                    // existing one from `const_assertions` crate, for the
                    // purpose of better errors reporting when the assertion
                    // fails.

                    let trait_with_hint = format_ident!(
                        "UseParameterNameInsteadOf{}",
                        to_pascal_case(name),
                    );

                    quote! {
                        // In case we encounter default parameter, we should
                        // assert that corresponding argument's type __doesn't__
                        // implement a `Parameter` trait.
                        // TODO: Try to use autoderef-based specialization with
                        //       readable assertion message.
                        #[automatically_derived]
                        const _: fn() = || {
                            // Generic trait with a blanket impl over `()` for
                            // all types.
                            #[automatically_derived]
                            trait #trait_with_hint<A> {
                                fn method() {}
                            }

                            #[automatically_derived]
                            impl<T: ?Sized> #trait_with_hint<()> for T {}

                            // Used for the specialized impl when `Parameter` is
                            // implemented.
                            #[automatically_derived]
                            #[allow(dead_code)]
                            struct Invalid;

                            #[automatically_derived]
                            impl<T: ?Sized + ::namako::Parameter>
                                #trait_with_hint<Invalid> for T {}

                            // If there is only one specialized trait impl, type
                            // inference with `_` can be resolved and this can
                            // compile. Fails to compile if `#ty` implements
                            // `#trait_with_hint<Invalid>`.
                            let _: fn() = <#ty as #trait_with_hint<_>>::method;
                        };
                    }
                } else {
                    // Here we use double escaping to properly render `{name}`
                    // in the assertion message of the generated code.
                    let assert_msg = format!(
                        "Type `{}` doesn't implement a custom parameter \
                         `{{{{{name}}}}}`",
                        quote! { #ty },
                    );

                    quote! {
                        // In case we encounter a custom parameter, we should
                        // assert that the corresponding type implements
                        // `Parameter` and has correct `Parameter::NAME`.
                        #[automatically_derived]
                        const _: () = ::std::assert!(
                            ::namako::codegen::str_eq(
                                <#ty as ::namako::Parameter>::NAME,
                                #name,
                            ),
                            #assert_msg,
                        );
                    }
                }
            })
            .collect()
    }

    /// Generates code implementing a [`Provider`] for the given `ty`pe.
    ///
    /// [`Provider`]: cucumber_expressions::expand::parameters::Provider
    fn gen_provider_impl(&self, ty: &syn::Type) -> TokenStream {
        let (custom_par, custom_par_ty): (Vec<_>, Vec<_>) = self
            .0
            .iter()
            .filter_map(|par| {
                let name = par.param.input.fragment();
                (!DEFAULT_PARAMETERS.contains(name)).then_some((*name, &par.ty))
            })
            .unzip();

        quote! {
            #[automatically_derived]
            impl<'s> ::namako::codegen::ParametersProvider<
                ::namako::codegen::Spanned<'s>
            > for #ty {
                type Item = char;
                type Value = &'static str;

                fn get(
                    &self,
                    input: &::namako::codegen::Spanned<'s>,
                ) -> ::std::option::Option<Self::Value> {
                    #( if *input.fragment() == #custom_par {
                        ::std::option::Option::Some(
                            <#custom_par_ty as ::namako::Parameter>::REGEX,
                        )
                    } else )* {
                        ::std::option::Option::None
                    }
                }
            }
        }
    }
}

/// Argument of the attribute macro.
#[derive(Clone, Debug)]
enum AttributeArgument {
    /// `#[step("namako-expression")]` case.
    Expression(syn::LitStr),
}

impl Parse for AttributeArgument {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let arg = input.parse::<syn::LitStr>()?;
        Ok(Self::Expression(arg))
    }
}

/// Removes all `#[attr_arg]` attributes from the given function signature and
/// returns these attributes along with the corresponding function's arguments
/// in case there are no more `#[given]`, `#[when]` or `#[then]` attributes.
fn remove_all_attrs_if_needed<'a>(
    attr_arg: &str,
    func: &'a mut syn::ItemFn,
) -> (Vec<&'a syn::FnArg>, Vec<syn::Attribute>) {
    let has_other_step_arguments = func.attrs.iter().any(|attr| {
        attr.meta.path().segments.last().is_some_and(|segment| {
            ["given", "when", "then"].iter().any(|step| segment.ident == step)
        })
    });

    func.sig
        .inputs
        .iter_mut()
        .filter_map(|arg| {
            if has_other_step_arguments {
                find_attr(attr_arg, arg)
            } else {
                remove_attr(attr_arg, arg)
            }
            .map(move |attr| (&*arg, attr))
        })
        .unzip()
}

/// Finds attribute `#[attr_arg]` from function's argument, if any.
fn find_attr(attr_arg: &str, arg: &mut syn::FnArg) -> Option<syn::Attribute> {
    if let syn::FnArg::Typed(typed_arg) = arg {
        typed_arg
            .attrs
            .iter()
            .find(|attr| {
                attr.meta
                    .path()
                    .get_ident()
                    .is_some_and(|ident| ident == attr_arg)
            })
            .cloned()
    } else {
        None
    }
}

/// Removes attribute `#[attr_arg]` from function's argument, if any.
fn remove_attr(attr_arg: &str, arg: &mut syn::FnArg) -> Option<syn::Attribute> {
    use itertools::{Either, Itertools as _};

    if let syn::FnArg::Typed(typed_arg) = arg {
        let attrs = mem::take(&mut typed_arg.attrs);

        let (mut other, mut removed): (Vec<_>, Vec<_>) =
            attrs.into_iter().partition_map(|attr| {
                if let Some(ident) = attr.meta.path().get_ident()
                    && ident == attr_arg
                {
                    return Either::Right(attr);
                }
                Either::Left(attr)
            });

        if removed.len() == 1 {
            typed_arg.attrs = other;
            return removed.pop();
        }
        other.append(&mut removed);
        typed_arg.attrs = other;
    }
    None
}

/// Parses [`syn::Ident`] and [`syn::Type`] from the given [`syn::FnArg`].
///
/// [`syn::Ident`]: struct@syn::Ident
fn parse_fn_arg(arg: &syn::FnArg) -> syn::Result<(&syn::Ident, &syn::Type)> {
    let arg = match arg {
        syn::FnArg::Typed(t) => t,
        syn::FnArg::Receiver(_) => {
            return Err(syn::Error::new(
                arg.span(),
                "expected regular argument, found `self`",
            ));
        }
    };

    let syn::Pat::Ident(syn::PatIdent { ident, .. }) = arg.pat.as_ref() else {
        return Err(syn::Error::new(arg.span(), "expected ident"));
    };

    Ok((ident, arg.ty.as_ref()))
}

/// Parses type of a first slice element of the given function signature.
fn find_first_slice(sig: &syn::Signature) -> Option<&syn::TypePath> {
    sig.inputs.iter().find_map(|arg| {
        let typed_arg = match arg {
            syn::FnArg::Typed(typed_arg) => typed_arg,
            syn::FnArg::Receiver(_) => return None,
        };
        let syn::Type::Reference(ty_ref) = typed_arg.ty.as_ref() else {
            return None;
        };
        let syn::Type::Slice(slice) = ty_ref.elem.as_ref() else {
            return None;
        };
        if let syn::Type::Path(ty) = slice.elem.as_ref() {
            Some(ty)
        } else {
            None
        }
    })
}

/// Parses the context type and determines whether lifetime injection is needed.
///
/// Two patterns are supported for step function signatures:
///
/// 1. **Reference patterns** (simple worlds): `&mut World` or `&World`
///    - The reference type itself is the context type
///    - Blanket impl `impl<W: World> StepContext for &mut W` handles these
///    - No lifetime injection needed (`needs_lifetime = false`)
///
/// 2. **Wrapper patterns** (complex worlds): `TestWorldMut` or `TestWorldRef`
///    - A custom wrapper type that implements `StepContext`
///    - Requires lifetime injection: `TestWorldMut` → `TestWorldMut<'__ctx>`
///    - (`needs_lifetime = true`)
///
/// Returns `(ctx_type, needs_lifetime)` where:
/// - `ctx_type`: The type to use for `StepContext` trait lookup
/// - `needs_lifetime`: Whether the macro should inject a lifetime parameter
fn parse_context_type_and_mode(
    sig: &Signature,
    attr_name: &str,
) -> syn::Result<(syn::Type, bool)> {
    let first_arg = sig.inputs.first().ok_or_else(|| {
        syn::Error::new(
            sig.ident.span(),
            "step function must have at least one argument (the context)",
        )
    })?;

    let typed_arg = match first_arg {
        syn::FnArg::Typed(a) => a,
        syn::FnArg::Receiver(r) => {
            return Err(syn::Error::new(r.span(), "step function cannot use `self`"));
        }
    };

    match typed_arg.ty.as_ref() {
        // Reference type like `&mut World` or `&mut TestWorldMut`
        syn::Type::Reference(r) => {
            // Validate mutability based on step kind
            if attr_name == "then" {
                if r.mutability.is_some() {
                    return Err(syn::Error::new(
                        r.span(),
                        "Then steps should use `&ContextType`, not `&mut ContextType`",
                    ));
                }
            } else {
                // Given/When
                if r.mutability.is_none() {
                    return Err(syn::Error::new(
                        r.span(),
                        "Given/When steps should use `&mut ContextType`, not `&ContextType`",
                    ));
                }
            }

            // Extract the inner type path
            if let syn::Type::Path(inner_p) = r.elem.as_ref() {
                // Check that the inner type has no explicit generics
                // Check for existing generics - but allow '__ctx which means already processed
                if let Some(last_segment) = inner_p.path.segments.last() {
                    if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                        // Check if this is our injected '__ctx lifetime
                        let is_our_lifetime = args.args.iter().any(|arg| {
                            matches!(arg, syn::GenericArgument::Lifetime(lt) if lt.ident == "__ctx")
                        });
                        if !is_our_lifetime {
                            return Err(syn::Error::new(
                                last_segment.arguments.span(),
                                "context type must not have explicit lifetimes or generics",
                            ));
                        }
                    }
                }

                // Create the inner type with 'static lifetime for StepContext lookup
                // Works for wrapper types: `<TestWorldMut<'static> as StepContext>::World`
                let ctx_type_with_lifetime = {
                    let mut path = inner_p.path.clone();
                    if let Some(last_seg) = path.segments.last_mut() {
                        last_seg.arguments = syn::PathArguments::AngleBracketed(
                            syn::AngleBracketedGenericArguments {
                                colon2_token: None,
                                lt_token: Default::default(),
                                args: std::iter::once(syn::GenericArgument::Lifetime(
                                    syn::Lifetime::new("'static", proc_macro2::Span::call_site()),
                                ))
                                .collect(),
                                gt_token: Default::default(),
                            },
                        );
                    }
                    syn::Type::Path(syn::TypePath {
                        qself: inner_p.qself.clone(),
                        path,
                    })
                };

                // Needs lifetime injection to rewrite TestWorldMut -> TestWorldMut<'__ctx>
                Ok((ctx_type_with_lifetime, true))
            } else {
                Err(syn::Error::new(
                    r.elem.span(),
                    "expected a type path inside the reference",
                ))
            }
        }

        // Pattern 1: Bare type like `TestWorldMut` (needs lifetime injection)
        syn::Type::Path(p) => {
            // Check for existing generics - but allow '__ctx which means already processed
            if let Some(last_segment) = p.path.segments.last() {
                if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                    // Check if this is our injected '__ctx lifetime
                    let is_our_lifetime = args.args.iter().any(|arg| {
                        matches!(arg, syn::GenericArgument::Lifetime(lt) if lt.ident == "__ctx")
                    });
                    if !is_our_lifetime {
                        return Err(syn::Error::new(
                            last_segment.arguments.span(),
                            "context type must not have explicit lifetimes or generics; \
                             the macro injects them automatically",
                        ));
                    }
                }
            }

            // Create a version with 'static lifetime for StepContext lookup
            // (The actual lifetime doesn't matter - we only use the associated World type)
            let ctx_type_with_lifetime = {
                let mut path = p.path.clone();
                if let Some(last_seg) = path.segments.last_mut() {
                    last_seg.arguments = syn::PathArguments::AngleBracketed(
                        syn::AngleBracketedGenericArguments {
                            colon2_token: None,
                            lt_token: Default::default(),
                            args: std::iter::once(syn::GenericArgument::Lifetime(
                                syn::Lifetime::new("'static", proc_macro2::Span::call_site()),
                            ))
                            .collect(),
                            gt_token: Default::default(),
                        },
                    );
                }
                syn::Type::Path(syn::TypePath {
                    qself: p.qself.clone(),
                    path,
                })
            };

            Ok((ctx_type_with_lifetime, true))
        }

        _ => {
            let msg = if attr_name == "then" {
                "first argument must be `&World` (reference) or `ContextType` (wrapper)"
            } else {
                "first argument must be `&mut World` (reference) or `ContextType` (wrapper)"
            };
            Err(syn::Error::new(typed_arg.span(), msg))
        }
    }
}

// Keep the old function for backward compatibility with other code that may use it
#[allow(dead_code)]
fn parse_context_type_from_args<'a>(sig: &'a Signature, attr_name: &str) -> syn::Result<&'a syn::TypePath> {
    let first_arg = sig.inputs.first().ok_or_else(|| {
        let msg = if attr_name == "then" {
            "first function argument expected to be `TestWorldRef` (no generics) for `Then` steps"
        } else {
            "first function argument expected to be `mut TestWorldMut` (no generics) for `Given`/`When` steps"
        };
        syn::Error::new(sig.ident.span(), msg)
    })?;

    let typed_arg = match first_arg {
        syn::FnArg::Typed(a) => a,
        syn::FnArg::Receiver(r) => {
            return Err(syn::Error::new(r.span(), "step function cannot use `self`"));
        }
    };

    // Context-first ABI: accept a type path WITHOUT generics.
    // Users must write `TestWorldMut`, not `TestWorldMut<'a>`.
    if let syn::Type::Path(p) = typed_arg.ty.as_ref() {
        // Check that the type has NO generics (the ergonomics rule)
        if let Some(last_segment) = p.path.segments.last() {
            if !last_segment.arguments.is_empty() {
                return Err(syn::Error::new(
                    last_segment.arguments.span(),
                    "context type must not have explicit lifetimes or generics; \
                     write `TestWorldMut` or `TestWorldRef`, not `TestWorldMut<'a>`",
                ));
            }
        }
        Ok(p)
    } else if let syn::Type::Reference(r) = typed_arg.ty.as_ref() {
        // For Then steps: allow `&TestWorldRef` (immutable reference to context)
        // For Given/When steps: allow `&mut TestWorldMut` (mutable reference to context)
        if attr_name == "then" {
            // Then steps accept &TestWorldRef (immutable)
            if r.mutability.is_some() {
                return Err(syn::Error::new(
                    r.span(),
                    "Then steps should use `&TestWorldRef`, not `&mut TestWorldRef`",
                ));
            }
            // Extract the inner type path
            if let syn::Type::Path(inner_p) = r.elem.as_ref() {
                // Check no generics on inner type
                if let Some(last_segment) = inner_p.path.segments.last() {
                    if !last_segment.arguments.is_empty() {
                        return Err(syn::Error::new(
                            last_segment.arguments.span(),
                            "context type must not have explicit lifetimes or generics; \
                             write `&TestWorldRef`, not `&TestWorldRef<'a>`",
                        ));
                    }
                }
                Ok(inner_p)
            } else {
                Err(syn::Error::new(
                    r.elem.span(),
                    "Then steps should use `&TestWorldRef`",
                ))
            }
        } else {
            // Given/When steps: allow `&mut TestWorldMut` (mutable reference)
            if r.mutability.is_none() {
                return Err(syn::Error::new(
                    r.span(),
                    "Given/When steps should use `&mut TestWorldMut`, not `&TestWorldMut`",
                ));
            }
            // Extract the inner type path
            if let syn::Type::Path(inner_p) = r.elem.as_ref() {
                // Check no generics on inner type
                if let Some(last_segment) = inner_p.path.segments.last() {
                    if !last_segment.arguments.is_empty() {
                        return Err(syn::Error::new(
                            last_segment.arguments.span(),
                            "context type must not have explicit lifetimes or generics; \
                             write `&mut TestWorldMut`, not `&mut TestWorldMut<'a>`",
                        ));
                    }
                }
                Ok(inner_p)
            } else {
                Err(syn::Error::new(
                    r.elem.span(),
                    "Given/When steps should use `&mut TestWorldMut`",
                ))
            }
        }
    } else {
        let msg = if attr_name == "then" {
            "first function argument expected to be `&TestWorldRef` (no generics) for `Then` steps"
        } else {
            "first function argument expected to be `ctx: &mut TestWorldMut` (no generics) for `Given`/`When` steps"
        };
        Err(syn::Error::new(typed_arg.span(), msg))
    }
}

/// Rewrites the function signature to inject a fresh lifetime for context-first ABI.
///
/// Transforms context types that accept lifetimes:
///   `fn step(ctx: &mut TestWorldMut, name: String) { ... }`
/// Into:
///   `fn step<'__ctx>(ctx: &mut TestWorldMut<'__ctx>, name: String) { ... }`
///
/// For simple world types that don't accept lifetimes (like `&mut World`),
/// the signature is left unchanged - no lifetime injection is needed.
///
/// This provides ergonomic step authoring without requiring explicit lifetimes.
fn rewrite_signature_with_lifetime(func: &mut syn::ItemFn, needs_lifetime: bool) -> syn::Result<()> {
    // Only inject lifetime if the context type needs it
    if !needs_lifetime {
        return Ok(());
    }

    // Get the first argument type
    let first_arg = func.sig.inputs.first_mut().ok_or_else(|| {
        syn::Error::new(func.sig.ident.span(), "step function must have at least one argument")
    })?;

    let typed_arg = match first_arg {
        syn::FnArg::Typed(t) => t,
        syn::FnArg::Receiver(r) => {
            return Err(syn::Error::new(r.span(), "step function cannot use `self`"));
        }
    };

    // Handle both Type::Path and Type::Reference
    let type_path = match typed_arg.ty.as_mut() {
        syn::Type::Path(p) => p,
        syn::Type::Reference(r) => {
            // For references like &TestWorldRef, we need to rewrite the inner type
            if let syn::Type::Path(inner_p) = r.elem.as_mut() {
                inner_p
            } else {
                // Not a path inside the reference
                return Ok(());
            }
        }
        _ => {
            // Not a type we handle
            return Ok(());
        }
    };

    let last_segment = match type_path.path.segments.last_mut() {
        Some(s) => s,
        None => return Ok(()),
    };

    // Check if already processed by a previous attribute (e.g., #[given] + #[when] on same fn)
    // If we see `'__ctx` lifetime, skip injection - it's already been done
    if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
        for arg in &args.args {
            if let syn::GenericArgument::Lifetime(lt) = arg {
                if lt.ident == "__ctx" {
                    // Already processed, skip
                    return Ok(());
                }
            }
        }
        // Has generics but not our marker - user wrote them explicitly
        return Err(syn::Error::new(
            last_segment.arguments.span(),
            "context type must not have explicit lifetimes; the macro injects them automatically",
        ));
    }

    // Add the fresh lifetime to the function generics (only if not already present)
    let ctx_lifetime: syn::Lifetime = syn::parse_quote!('__ctx);
    let has_ctx_lifetime = func.sig.generics.params.iter().any(|p| {
        matches!(p, syn::GenericParam::Lifetime(lt) if lt.lifetime.ident == "__ctx")
    });
    if !has_ctx_lifetime {
        func.sig.generics.params.insert(
            0,
            syn::GenericParam::Lifetime(syn::LifetimeParam::new(ctx_lifetime.clone())),
        );
    }

    // Rewrite the type to include the lifetime: TestWorldMut -> TestWorldMut<'__ctx>
    last_segment.arguments = syn::PathArguments::AngleBracketed(
        syn::AngleBracketedGenericArguments {
            colon2_token: None,
            lt_token: syn::token::Lt::default(),
            args: std::iter::once(syn::GenericArgument::Lifetime(ctx_lifetime))
                .collect(),
            gt_token: syn::token::Gt::default(),
        },
    );

    Ok(())
}

// =============================================================================
// NPAP v1 Signature Analysis
// =============================================================================

/// NPAP v1 signature information extracted from a step function.
struct SignatureInfo {
    /// Number of capture parameters (excluding World, Step context, DocString, DataTable).
    captures_arity: u32,
    /// Whether the function accepts a DocString parameter.
    accepts_docstring: bool,
    /// Whether the function accepts a DataTable parameter.
    accepts_datatable: bool,
}

/// Checks if a type represents a DocString parameter.
///
/// Per GOLD_PLAN §4.4.3, DocString is typically `Option<String>` or a wrapper type.
fn is_docstring_type(ty: &syn::Type) -> bool {
    // Check for Option<String>
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(syn::Type::Path(inner))) =
                        args.args.first()
                    {
                        if let Some(inner_seg) = inner.path.segments.last() {
                            // Option<String> or Option<DocString>
                            return inner_seg.ident == "String"
                                || inner_seg.ident == "DocString";
                        }
                    }
                }
            }
            // Direct DocString type
            if segment.ident == "DocString" {
                return true;
            }
        }
    }
    false
}

/// Checks if a type represents a DataTable parameter.
///
/// Per GOLD_PLAN §4.4.4, DataTable is typically `Option<Vec<Vec<String>>>` or a wrapper.
fn is_datatable_type(ty: &syn::Type) -> bool {
    // Check for Option<Vec<Vec<String>>> or DataTable wrapper
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(syn::Type::Path(inner))) =
                        args.args.first()
                    {
                        if let Some(inner_seg) = inner.path.segments.last() {
                            // Check for Vec<Vec<String>> or DataTable
                            if inner_seg.ident == "Vec" || inner_seg.ident == "DataTable" {
                                // For simplicity, if it's Option<Vec<...>> after DocString detection,
                                // assume it's a DataTable candidate
                                return true;
                            }
                        }
                    }
                }
            }
            // Direct DataTable type
            if segment.ident == "DataTable" {
                return true;
            }
        }
    }
    false
}
