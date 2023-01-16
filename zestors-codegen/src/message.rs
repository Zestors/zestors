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
            type Returned = <#returns as zestors::messaging::MessageDerive<Self>>::Returned;
            type Payload = <#returns as zestors::messaging::MessageDerive<Self>>::Payload;
            fn create(self) -> (Self::Payload, Self::Returned) {
                <#returns as zestors::messaging::MessageDerive<Self>>::create(self)
            }
            fn cancel(payload: Self::Payload, returned: Self::Returned) -> Self {
                <#returns as zestors::messaging::MessageDerive<Self>>::cancel(payload, returned)
            }
        }
    })
}
