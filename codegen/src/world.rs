//! `#[derive(World)]` macro implementation.

use inflections::case::to_pascal_case;
use itertools::Itertools as _;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse_quote;
use synthez::{ParseAttrs, ToTokens};

/// Generates code of `#[derive(World)]` macro expansion.
///
/// # Errors
///
/// If failed to parse [`Attrs`].
pub(crate) fn derive(input: TokenStream) -> syn::Result<TokenStream> {
    let input = syn::parse2::<syn::DeriveInput>(input)?;
    let definition = Definition::try_from(input)?;

    Ok(quote! { #definition })
}

/// Helper attributes of `#[derive(World)]` macro.
#[derive(Debug, Default, ParseAttrs)]
struct Attrs {
    /// Function to be used for a `World` construction.
    ///
    /// If [`None`] then [`Default::default()`] will be used.
    #[parse(value)]
    init: Option<syn::ExprPath>,

    /// Type to use for `MutCtx` associated type.
    ///
    /// Required for context-first ABI.
    #[parse(value)]
    mut_ctx: Option<syn::Type>,

    /// Type to use for `RefCtx` associated type.
    ///
    /// Required for context-first ABI.
    #[parse(value)]
    ref_ctx: Option<syn::Type>,

    /// Expression to create `MutCtx` from `&mut self`.
    ///
    /// Defaults to `Self::MutCtx::new(self)` if not specified.
    #[parse(value)]
    ctx_mut: Option<syn::ExprPath>,

    /// Expression to create `RefCtx` from `&self`.
    ///
    /// Defaults to `Self::RefCtx::new(self)` if not specified.
    #[parse(value)]
    ctx_ref: Option<syn::ExprPath>,
}

/// Representation of a type implementing a `World` trait, used for code
/// generation.
#[derive(Debug, ToTokens)]
#[to_tokens(append(
    impl_world_inventory,
    impl_world,
    impl_step_contexts,
    impl_step_constructors
))]
struct Definition {
    /// Name of this type.
    ident: syn::Ident,

    /// [`syn::Generics`] of this type.
    generics: syn::Generics,

    /// [`Visibility`] of this `World`.
    ///
    /// [`Visibility`]: syn::Visibility
    vis: syn::Visibility,

    /// Function, which is used to construct `World`. Uses [`Default`] impl, in
    /// case no value is provided.
    init: Option<syn::ExprPath>,

    /// Type to use for `MutCtx` associated type.
    mut_ctx: Option<syn::Type>,

    /// Type to use for `RefCtx` associated type.
    ref_ctx: Option<syn::Type>,

    /// Expression to create `MutCtx` from `&mut self`.
    ctx_mut: Option<syn::ExprPath>,

    /// Expression to create `RefCtx` from `&self`.
    ctx_ref: Option<syn::ExprPath>,
}

impl TryFrom<syn::DeriveInput> for Definition {
    type Error = syn::Error;

    fn try_from(input: syn::DeriveInput) -> syn::Result<Self> {
        let attrs: Attrs = Attrs::parse_attrs("world", &input)?;

        Ok(Self {
            ident: input.ident,
            generics: input.generics,
            vis: input.vis,
            init: attrs.init,
            mut_ctx: attrs.mut_ctx,
            ref_ctx: attrs.ref_ctx,
            ctx_mut: attrs.ctx_mut,
            ctx_ref: attrs.ctx_ref,
        })
    }
}

impl Definition {
    /// Possible step names.
    const STEPS: &'static [&'static str] = &["given", "when", "then"];

    /// Assertion to ensure, that [`Self::STEPS`] has exactly 3 step types.
    #[expect(clippy::manual_assert, reason = "`assert_eq!` isn't const yet")]
    const EXACTLY_3_STEPS: () = if Self::STEPS.len() != 3 {
        panic!("expected exactly 3 step names");
    };

    /// Generates code of implementing a `WorldInventory` trait.
    fn impl_world_inventory(&self) -> TokenStream {
        let world = &self.ident;
        let (impl_gens, ty_gens, where_clause) = self.generics.split_for_impl();

        let (given_ty, when_step_ty, then_ty) = self
            .step_types()
            .collect_tuple()
            .unwrap_or_else(|| unreachable!("{:?}", Self::EXACTLY_3_STEPS));

        quote! {
            #[automatically_derived]
            impl #impl_gens ::namako_engine::codegen::WorldInventory
                 for #world #ty_gens
                 #where_clause
            {
                type Given = #given_ty;
                type When = #when_step_ty;
                type Then = #then_ty;
            }
        }
    }

    /// Generates code of implementing a `World` trait.
    fn impl_world(&self) -> TokenStream {
        let world = &self.ident;
        let (impl_gens, ty_gens, where_clause) = self.generics.split_for_impl();

        let init = self
            .init
            .clone()
            .unwrap_or_else(|| parse_quote! { <Self as ::std::default::Default>::default });

        // Context types - required for context-first ABI
        let mut_ctx_ty = self
            .mut_ctx
            .clone()
            .unwrap_or_else(|| parse_quote! { &'a mut Self });
        let ref_ctx_ty = self
            .ref_ctx
            .clone()
            .unwrap_or_else(|| parse_quote! { &'a Self });

        // Context factory expressions
        let ctx_mut_expr = if let Some(expr) = &self.ctx_mut {
            quote! { #expr(self) }
        } else if self.mut_ctx.is_some() {
            // If mut_ctx is specified, assume it has a `new` constructor
            quote! { <Self::MutCtx<'_>>::new(self) }
        } else {
            // Default: return &mut self
            quote! { self }
        };

        let ctx_ref_expr = if let Some(expr) = &self.ctx_ref {
            quote! { #expr(self) }
        } else if self.ref_ctx.is_some() {
            // If ref_ctx is specified, assume it has a `new` constructor
            quote! { <Self::RefCtx<'_>>::new(self) }
        } else {
            // Default: return &self
            quote! { self }
        };

        quote! {
            #[automatically_derived]
            impl #impl_gens ::namako_engine::World for #world #ty_gens
                 #where_clause
            {
                type Error = ::namako_engine::codegen::anyhow::Error;

                type MutCtx<'a> = #mut_ctx_ty where Self: 'a;
                type RefCtx<'a> = #ref_ctx_ty where Self: 'a;

                async fn new() -> ::std::result::Result<Self, Self::Error> {
                    use ::namako_engine::codegen::{
                        IntoWorldResult as _, ToWorldFuture as _,
                    };

                    fn as_fn_ptr<T>(v: fn() -> T) -> fn() -> T {
                        v
                    }

                    (&as_fn_ptr(#init))
                        .to_world_future()
                        .await
                        .into_world_result()
                        .map_err(::std::convert::Into::into)
                }

                fn ctx_mut(&mut self) -> Self::MutCtx<'_> {
                    #ctx_mut_expr
                }

                fn ctx_ref(&mut self) -> Self::RefCtx<'_> {
                    #ctx_ref_expr
                }
            }
        }
    }

    /// Generates `StepContext` impls for the context wrapper types.
    ///
    /// This eliminates the need for users to manually implement `StepContext`
    /// for their context wrapper types. The derive macro knows the context
    /// types from `#[world(mut_ctx = ..., ref_ctx = ...)]` and can generate
    /// the impls automatically.
    #[must_use]
    fn impl_step_contexts(&self) -> TokenStream {
        let world = &self.ident;
        let (_, ty_gens, _) = self.generics.split_for_impl();

        let mut impls = TokenStream::new();

        // Generate StepContext impl for mut_ctx type
        if let Some(ref mut_ctx_ty) = self.mut_ctx {
            if let Some(impl_tokens) = Self::generate_step_context_impl(mut_ctx_ty, world, &ty_gens)
            {
                impls.extend(impl_tokens);
            }
        }

        // Generate StepContext impl for ref_ctx type (only if different from mut_ctx)
        if let Some(ref ref_ctx_ty) = self.ref_ctx {
            // Check if ref_ctx is the same as mut_ctx to avoid duplicate impls
            let is_same = self.mut_ctx.as_ref().map_or(false, |mut_ty| {
                // Compare type string representations for simplicity
                quote!(#mut_ty).to_string() == quote!(#ref_ctx_ty).to_string()
            });

            if !is_same {
                if let Some(impl_tokens) =
                    Self::generate_step_context_impl(ref_ctx_ty, world, &ty_gens)
                {
                    impls.extend(impl_tokens);
                }
            }
        }

        impls
    }

    /// Generate a `StepContext` impl for a context type.
    ///
    /// Handles types like `WorldMut<'a>` by extracting the base path and
    /// generating an impl with a wildcard lifetime.
    fn generate_step_context_impl(
        ctx_ty: &syn::Type,
        world: &syn::Ident,
        world_ty_gens: &syn::TypeGenerics<'_>,
    ) -> Option<TokenStream> {
        match ctx_ty {
            syn::Type::Path(type_path) => {
                // Type like `WorldMut<'a>` - extract base path
                let path = &type_path.path;

                // Check if it has generic arguments (lifetime)
                if let Some(last_segment) = path.segments.last() {
                    if matches!(
                        last_segment.arguments,
                        syn::PathArguments::AngleBracketed(_)
                    ) {
                        // Has generics - generate impl with wildcard lifetime
                        // e.g., `impl StepContext for WorldMut<'_>`
                        let base_path = {
                            let mut p = path.clone();
                            if let Some(seg) = p.segments.last_mut() {
                                seg.arguments = syn::PathArguments::AngleBracketed(
                                    syn::AngleBracketedGenericArguments {
                                        colon2_token: None,
                                        lt_token: Default::default(),
                                        args: std::iter::once(syn::GenericArgument::Lifetime(
                                            syn::Lifetime::new(
                                                "'_",
                                                proc_macro2::Span::call_site(),
                                            ),
                                        ))
                                        .collect(),
                                        gt_token: Default::default(),
                                    },
                                );
                            }
                            p
                        };

                        return Some(quote! {
                            #[automatically_derived]
                            impl ::namako_engine::codegen::StepContext for #base_path {
                                type World = #world #world_ty_gens;
                            }
                        });
                    }
                }

                // No generics - generate simple impl
                Some(quote! {
                    #[automatically_derived]
                    impl ::namako_engine::codegen::StepContext for #path {
                        type World = #world #world_ty_gens;
                    }
                })
            }
            // Reference types are not wrapper types, skip them
            syn::Type::Reference(_) => None,
            _ => None,
        }
    }

    /// Generates code for additional struct implementing `StepConstructor`
    /// trait.
    #[must_use]
    fn impl_step_constructors(&self) -> TokenStream {
        let world = &self.ident;
        let world_vis = &self.vis;
        let (impl_gens, ty_gens, where_clause) = self.generics.split_for_impl();

        self.step_types()
            .map(|ty| {
                quote! {
                    #[automatically_derived]
                    #[doc(hidden)]
                    #world_vis struct #ty {
                        #[doc(hidden)]
                        #world_vis loc: ::namako_engine::step::Location,

                        // NPAP v1 metadata fields
                        #[doc(hidden)]
                        #world_vis binding_id: &'static str,

                        #[doc(hidden)]
                        #world_vis expression: &'static str,

                        #[doc(hidden)]
                        #world_vis kind: &'static str,

                        #[doc(hidden)]
                        #world_vis impl_hash: &'static str,

                        #[doc(hidden)]
                        #world_vis captures_arity: u32,

                        #[doc(hidden)]
                        #world_vis accepts_docstring: bool,

                        #[doc(hidden)]
                        #world_vis accepts_datatable: bool,

                        /// Source symbol: stable identifier for the binding.
                        /// Format: `module::path::function_name` (via module_path!())
                        #[doc(hidden)]
                        #world_vis source_symbol: &'static str,

                        #[doc(hidden)]
                        #world_vis regex: ::namako_engine::codegen::LazyRegex,

                        #[doc(hidden)]
                        #world_vis func: ::namako_engine::Step<#world>,
                    }

                    #[automatically_derived]
                    impl #impl_gens
                         ::namako_engine::codegen::StepConstructor<#world #ty_gens>
                         for #ty #where_clause
                    {
                        fn inner(&self) -> (
                            ::namako_engine::step::Location,
                            ::namako_engine::codegen::LazyRegex,
                            ::namako_engine::Step<#world>,
                        ) {
                            (self.loc, self.regex, self.func)
                        }

                        fn npap_metadata(&self) -> ::namako_engine::codegen::NpapBindingMetadata {
                            ::namako_engine::codegen::NpapBindingMetadata {
                                binding_id: self.binding_id,
                                expression: self.expression,
                                kind: self.kind,
                                impl_hash: self.impl_hash,
                                captures_arity: self.captures_arity,
                                accepts_docstring: self.accepts_docstring,
                                accepts_datatable: self.accepts_datatable,
                                source_symbol: self.source_symbol,
                            }
                        }
                    }

                    #[automatically_derived]
                    ::namako_engine::codegen::collect!(#ty);
                }
            })
            .collect()
    }

    /// Generates [`syn::Ident`]s of generic types for private trait impl.
    ///
    /// [`syn::Ident`]: struct@syn::Ident
    fn step_types(&self) -> impl Iterator<Item = syn::Ident> {
        Self::STEPS
            .iter()
            .map(|step| format_ident!("Namako{}{}", to_pascal_case(step), self.ident))
    }
}

#[cfg(test)]
mod spec {
    use syn::parse_quote;

    #[test]
    fn derives_impl() {
        let input = parse_quote! {
            pub struct World;
        };

        // Just verify it compiles and produces non-empty output
        let result = super::derive(input).unwrap();
        let result_str = result.to_string();

        // Check key elements are present
        assert!(
            result_str.contains("WorldInventory"),
            "should implement WorldInventory"
        );
        assert!(
            result_str.contains("NamakoGivenWorld"),
            "should define Given step struct"
        );
        assert!(
            result_str.contains("NamakoWhenWorld"),
            "should define When step struct"
        );
        assert!(
            result_str.contains("NamakoThenWorld"),
            "should define Then step struct"
        );
        assert!(
            result_str.contains("binding_id"),
            "should include binding_id field"
        );
        assert!(
            result_str.contains("npap_metadata"),
            "should include npap_metadata method"
        );
    }

    #[test]
    fn derives_impl_with_generics() {
        let input = parse_quote! {
            pub struct World<T>(T);
        };

        let result = super::derive(input).unwrap();
        let result_str = result.to_string();

        assert!(
            result_str.contains("impl < T >"),
            "should have generic impl"
        );
        assert!(
            result_str.contains("WorldInventory"),
            "should implement WorldInventory"
        );
        assert!(
            result_str.contains("binding_id"),
            "should include binding_id field"
        );
    }

    #[test]
    fn derives_impl_with_init_fn() {
        let input = parse_quote! {
            #[world(init = Self::custom)]
            pub struct World<T>(T);
        };

        let result = super::derive(input).unwrap();
        let result_str = result.to_string();

        assert!(
            result_str.contains("Self :: custom"),
            "should use custom init"
        );
        assert!(
            result_str.contains("WorldInventory"),
            "should implement WorldInventory"
        );
        assert!(
            result_str.contains("npap_metadata"),
            "should include npap_metadata method"
        );
    }
}
