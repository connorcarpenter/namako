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
}

/// Representation of a type implementing a `World` trait, used for code
/// generation.
#[derive(Debug, ToTokens)]
#[to_tokens(append(impl_world_inventory, impl_world, impl_step_constructors))]
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
            impl #impl_gens ::namako::codegen::WorldInventory
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

        let init = self.init.clone().unwrap_or_else(
            || parse_quote! { <Self as ::std::default::Default>::default },
        );

        quote! {
            #[automatically_derived]
            impl #impl_gens ::namako::World for #world #ty_gens
                 #where_clause
            {
                type Error = ::namako::codegen::anyhow::Error;

                async fn new() -> ::std::result::Result<Self, Self::Error> {
                    use ::namako::codegen::{
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
            }
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
                        #world_vis loc: ::namako::step::Location,

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

                        #[doc(hidden)]
                        #world_vis regex: ::namako::codegen::LazyRegex,

                        #[doc(hidden)]
                        #world_vis func: ::namako::Step<#world>,
                    }

                    #[automatically_derived]
                    impl #impl_gens
                         ::namako::codegen::StepConstructor<#world #ty_gens>
                         for #ty #where_clause
                    {
                        fn inner(&self) -> (
                            ::namako::step::Location,
                            ::namako::codegen::LazyRegex,
                            ::namako::Step<#world>,
                        ) {
                            (self.loc, self.regex, self.func)
                        }

                        fn npap_metadata(&self) -> ::namako::codegen::NpapBindingMetadata {
                            ::namako::codegen::NpapBindingMetadata {
                                binding_id: self.binding_id,
                                expression: self.expression,
                                kind: self.kind,
                                impl_hash: self.impl_hash,
                                captures_arity: self.captures_arity,
                                accepts_docstring: self.accepts_docstring,
                                accepts_datatable: self.accepts_datatable,
                            }
                        }
                    }

                    #[automatically_derived]
                    ::namako::codegen::collect!(#ty);
                }
            })
            .collect()
    }

    /// Generates [`syn::Ident`]s of generic types for private trait impl.
    ///
    /// [`syn::Ident`]: struct@syn::Ident
    fn step_types(&self) -> impl Iterator<Item = syn::Ident> {
        Self::STEPS.iter().map(|step| {
            format_ident!("Namako{}{}", to_pascal_case(step), self.ident)
        })
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
        assert!(result_str.contains("WorldInventory"), "should implement WorldInventory");
        assert!(result_str.contains("NamakoGivenWorld"), "should define Given step struct");
        assert!(result_str.contains("NamakoWhenWorld"), "should define When step struct");
        assert!(result_str.contains("NamakoThenWorld"), "should define Then step struct");
        assert!(result_str.contains("binding_id"), "should include binding_id field");
        assert!(result_str.contains("npap_metadata"), "should include npap_metadata method");
    }

    #[test]
    fn derives_impl_with_generics() {
        let input = parse_quote! {
            pub struct World<T>(T);
        };

        let result = super::derive(input).unwrap();
        let result_str = result.to_string();

        assert!(result_str.contains("impl < T >"), "should have generic impl");
        assert!(result_str.contains("WorldInventory"), "should implement WorldInventory");
        assert!(result_str.contains("binding_id"), "should include binding_id field");
    }

    #[test]
    fn derives_impl_with_init_fn() {
        let input = parse_quote! {
            #[world(init = Self::custom)]
            pub struct World<T>(T);
        };

        let result = super::derive(input).unwrap();
        let result_str = result.to_string();

        assert!(result_str.contains("Self :: custom"), "should use custom init");
        assert!(result_str.contains("WorldInventory"), "should implement WorldInventory");
        assert!(result_str.contains("npap_metadata"), "should include npap_metadata method");
    }
}
