use heck::ToSnakeCase;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    parse::Parse, parse2, parse_quote, punctuated::Punctuated, spanned::Spanned, token::Colon,
    Attribute, Error, Field, Fields, FnArg, Generics, Item, Meta, Pat, PatIdent, PatType, Token,
};

pub fn derive_envelope(item: TokenStream) -> Result<TokenStream, Error> {
    let item = match parse2::<Item>(item)? {
        Item::Enum(item) => Err(Error::new_spanned(
            item,
            "Only implemented for structs right now",
        ))?,
        Item::Struct(item) => item,
        item => Err(Error::new_spanned(item, "Must be enum, struct"))?,
    };

    let ident = item.ident;
    let vis = item.vis;
    let (field_params, field_idents) = parse_fields(item.fields);
    let (trait_ident, method_ident) = match item
        .attrs
        .iter()
        .find(|attr| attr.path.is_ident("envelope"))
    {
        Some(attr) => {
            let Meta::List(meta_list) = attr.parse_meta()? else { panic!() };
            let mut iter = meta_list.nested.into_iter();
            let trait_name = iter.next().unwrap().to_token_stream();
            let fn_name = iter.next().unwrap().to_token_stream();
            (parse2(trait_name)?, parse2(fn_name)?)
        }
        None => {
            let trait_ident = Ident::new(&format!("{}Envelope", ident.to_string()), ident.span());
            let method_ident = Ident::new(&ident.to_string().to_snake_case(), ident.span());
            (trait_ident, method_ident)
        }
    };
    let trait_doc = format!(
        "
        Automatically generated trait for creating envelopes of the 
        message[`{ident}`].
        "
    );
    let method_doc = format!(
        "
        Creates an [`Envelope`](zestors::messaging::Envelope) for the 
        message[`{ident}`].
        "
    );

    Ok(quote! {
        #[doc = #trait_doc]
        #vis trait #trait_ident: zestors::monitoring::ActorRef
        where
            Self::ActorType: zestors::messaging::Accept<#ident>,
        {
            #[doc = #method_doc]
            fn #method_ident(
                &self,
                #field_params
            ) -> zestors::messaging::Envelope<'_, Self::ActorType, #ident> {
                <Self as zestors::monitoring::ActorRefExt>::envelope(self, #ident {
                    #field_idents
                })
            }
        }

        impl<T> #trait_ident for T
        where
            T: zestors::monitoring::ActorRef,
            T::ActorType: zestors::messaging::Accept<#ident>
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
