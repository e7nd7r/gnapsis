//! Implementation of #[derive(Context)] proc-macro.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

pub fn derive_context_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "Context can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "Context can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    // Generate FromRef impl for each field type
    let impls = fields.iter().map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        let field_type = &field.ty;

        quote! {
            impl #impl_generics crate::FromRef<#name #ty_generics> for #field_type #where_clause {
                fn from_ref(ctx: &#name #ty_generics) -> Self {
                    ctx.#field_name.clone()
                }
            }
        }
    });

    let expanded = quote! {
        #(#impls)*
    };

    TokenStream::from(expanded)
}
