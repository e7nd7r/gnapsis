//! Implementation of #[derive(FromContext)] proc-macro.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

pub fn derive_from_context_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Parse #[from_context(Context = MyContext)] attribute
    let context_type = parse_context_type(&input);

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "FromContext can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "FromContext can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    // Generate field initializers
    let field_inits = fields.iter().map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        let field_type = &field.ty;

        quote! {
            #field_name: <#field_type as crate::FromRef<#context_type>>::from_ref(ctx)
        }
    });

    let expanded = quote! {
        impl #impl_generics crate::FromRef<#context_type> for #name #ty_generics #where_clause {
            fn from_ref(ctx: &#context_type) -> Self {
                Self {
                    #(#field_inits),*
                }
            }
        }
    };

    TokenStream::from(expanded)
}

fn parse_context_type(input: &DeriveInput) -> proc_macro2::TokenStream {
    for attr in &input.attrs {
        if attr.path().is_ident("from_context") {
            // Parse #[from_context(Context = "MyContext")]
            let mut context_ty: Option<syn::Type> = None;

            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("Context") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    context_ty = Some(value.parse()?);
                }
                Ok(())
            });

            if let Some(ty) = context_ty {
                return quote! { #ty };
            }
        }
    }

    // Default to `Context`
    quote! { Context }
}
