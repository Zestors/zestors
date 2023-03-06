use heck::ToSnakeCase;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    parse2, parse_quote, punctuated::Punctuated, spanned::Spanned, token::Colon, Error, Fields,
    ItemStruct, Meta, Pat, PatType, Token,
};

pub fn derive_envelope(item: TokenStream) -> Result<TokenStream, Error> {
    let item = parse2::<ItemStruct>(item)?;

    if !item.generics.params.is_empty() {
        Err(Error::new_spanned(
            item.generics.params,
            "Generics currently not supported with Envelope",
        ))?
    }

    let (trait_name, method_name) = match item
        .attrs
        .iter()
        .find(|attr| attr.path.is_ident("envelope"))
    {
        Some(attr) => {
            let Meta::List(meta_list) = attr.parse_meta()? else { panic!() };
            let mut iter = meta_list.nested.into_iter();
            let Some(trait_name) = iter.next() else {
                Err(Error::new_spanned(&meta_list.path, "Expected trait name"))?
            };
            let Some(fn_name) = iter.next() else {
                Err(Error::new_spanned(&meta_list.path, "Expected method name"))?
            };
            (
                parse2(trait_name.to_token_stream())?,
                parse2(fn_name.to_token_stream())?,
            )
        }
        None => {
            let trait_ident = Ident::new(
                &format!("{}Envelope", item.ident.to_string()),
                item.ident.span(),
            );
            let method_ident =
                Ident::new(&item.ident.to_string().to_snake_case(), item.ident.span());
            (trait_ident, method_ident)
        }
    };
    let trait_doc = format!(
        "
        Automatically generated trait for creating envelopes of the 
        message[`{}`].
        ",
        item.ident
    );
    let method_doc = format!(
        "
        Creates an [`Envelope`](::zestors::messaging::Envelope) for the 
        message[`{}`].
        ",
        item.ident
    );
    let ident = item.ident;
    let vis = item.vis;
    let (field_params, field_idents) = parse_fields(item.fields);

    Ok(quote! {
        #[doc = #trait_doc]
        #vis trait #trait_name: ::zestors::actor_reference::ActorRef
        where
            Self::ActorType: ::zestors::messaging::Accepts<#ident>
        {
            #[doc = #method_doc]
            fn #method_name(
                &self,
                #field_params
            ) -> ::zestors::messaging::Envelope<'_, Self::ActorType, #ident> {
                <Self as ::zestors::actor_reference::ActorRefExt>::envelope(self, #ident {
                    #field_idents
                })
            }
        }

        impl<T> #trait_name for T
        where
            T: ::zestors::actor_reference::ActorRef,
            T::ActorType: ::zestors::messaging::Accepts<#ident>
        { }
    })
}

pub fn parse_fields(
    fields: Fields,
) -> (
    Punctuated<PatType, Token![,]>,
    Punctuated<Box<Pat>, Token![,]>,
) {
    let params = match fields {
        Fields::Named(named_fields) => named_fields
            .named
            .into_iter()
            .enumerate()
            .map(|(i, field)| {
                let span = field.span();
                let ident = field.ident.unwrap_or(Ident::new(&format!("arg{i}"), span));
                PatType {
                    attrs: field.attrs,
                    pat: parse_quote!(#ident),
                    colon_token: Colon(Span::call_site()),
                    ty: field.ty.into(),
                }
            })
            .collect::<Punctuated<_, Token![,]>>(),
        Fields::Unnamed(_) => todo!(),
        Fields::Unit => todo!(),
    };
    let fields = params
        .iter()
        .map(|field| field.pat.clone())
        .collect::<Punctuated<_, Token![,]>>();
    (params, fields)
}
