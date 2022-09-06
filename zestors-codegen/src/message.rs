use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::{parse::Parse, parse2, Attribute, Error, Generics, Item, Type};

struct DeriveMessage {
    ident: Ident,
    attrs: Vec<Attribute>,
    generics: Generics,
}

impl Parse for DeriveMessage {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        match input.parse::<Item>()? {
            Item::Enum(item) => Ok(DeriveMessage {
                ident: item.ident,
                attrs: item.attrs,
                generics: item.generics,
            }),
            Item::Struct(item) => Ok(DeriveMessage {
                ident: item.ident,
                attrs: item.attrs,
                generics: item.generics,
            }),
            Item::Union(item) => Ok(DeriveMessage {
                ident: item.ident,
                attrs: item.attrs,
                generics: item.generics,
            }),
            item => Err(Error::new_spanned(item, "Must be enum, struct or union")),
        }
    }
}

pub fn derive_message(item: TokenStream) -> Result<TokenStream, Error> {
    let DeriveMessage {
        ident,
        attrs,
        generics,
    } = parse2::<DeriveMessage>(item)?;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let returns = match attrs.iter().find(|attr| attr.path.is_ident("msg")) {
        Some(attr) => {
            let ty = attr.parse_args::<Type>()?;
            quote! { #ty }
        }
        None => quote! { () },
    };

    Ok(quote! {
        impl #impl_generics zestors::messaging::Message for #ident #ty_generics #where_clause {
            type Type = #returns;
        }
    })
}