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

    let msg_type = get_msg_type(&attrs)?;

    Ok(quote! {
        impl #impl_generics ::zestors::messaging::Message for #ident #ty_generics #where_clause {
            type Returned = <#msg_type as ::zestors::messaging::MessageDerive<Self>>::Returned;
            type Payload = <#msg_type as ::zestors::messaging::MessageDerive<Self>>::Payload;
            fn create(self) -> (Self::Payload, Self::Returned) {
                <#msg_type as ::zestors::messaging::MessageDerive<Self>>::create(self)
            }
            fn cancel(payload: Self::Payload, returned: Self::Returned) -> Self {
                <#msg_type as ::zestors::messaging::MessageDerive<Self>>::cancel(payload, returned)
            }
        }
    })
}

pub fn get_msg_type(attrs: &Vec<Attribute>) -> Result<TokenStream, Error> {
    let mut msg_type = if let Some(attr) = attrs.iter().find(|attr| attr.path.is_ident("msg")) {
        let ty = attr.parse_args::<Type>()?;
        Some(quote! { #ty })
    } else {
        None
    };

    if let Some(attr) = attrs.iter().find(|attr| attr.path.is_ident("request")) {
        if msg_type.is_some() {
            Err(Error::new_spanned(
                attr,
                "Can't have both #[msg(..)] and #[request(..)]",
            ))?
        }
        let ty = attr.parse_args::<Type>()?;
        msg_type = Some(quote! { ::zestors::messaging::Rx<#ty> })
    };

    Ok(msg_type.unwrap_or(quote! { () }))
}
