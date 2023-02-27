use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{
    parse2, parse_quote, punctuated::Punctuated, Error, Field, Fields, ItemEnum, Token, Type,
    Visibility,
};

pub fn protocol(_attr: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    let mut item_enum = parse2::<ItemEnum>(item)?;

    let protocol_msgs = modify_protocol_enum(&mut item_enum)?;
    let impl_protocol = impl_protocol(&item_enum, &protocol_msgs)?;
    let impl_accepts = impl_accepts(&item_enum, &protocol_msgs)?;
    let impl_handled_by = impl_handled_by(&item_enum, &protocol_msgs)?;

    Ok(quote! {
        #item_enum
        #impl_protocol
        #impl_handled_by
        #impl_accepts
    })
}

struct ProtocolMsg {
    enum_ident: Ident,
    msg_ty: Type,
}

fn impl_accepts(item: &ItemEnum, variants: &Vec<ProtocolMsg>) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    let accepts = variants.iter().map(|variant| {
            let variant_ty = &variant.msg_ty;
            let variant_ident = &variant.enum_ident;
            quote! {
                impl #impl_generics zestors::messaging::FromPayload<#variant_ty> for #ident #ty_generics #where_clause {
                    fn from_payload(
                        msg: <#variant_ty as zestors::messaging::Message>::Payload
                    ) -> Self {
                        Self::#variant_ident(msg)
                    }

                    fn try_into_payload(self) -> Result<
                        <#variant_ty as zestors::messaging::Message>::Payload,
                        Self
                    > {
                        match self {
                            Self::#variant_ident(msg) => Ok(msg),
                            prot => Err(prot)
                        }
                    }
                }
            }
        }).collect::<Vec<_>>();

    Ok(quote! {
        #(#accepts)*
    })
}

fn impl_protocol(item: &ItemEnum, variants: &Vec<ProtocolMsg>) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    let downcasts = variants
        .iter()
        .map(|variant| {
            let variant_ident = &variant.enum_ident;
            let variant_ty = &variant.msg_ty;
            quote! {
                let boxed = match boxed.downcast::<#variant_ty>() {
                    Ok(msg) => {
                        return Ok(Self::#variant_ident(msg));
                    }
                    Err(e) => e,
                };
            }
        })
        .collect::<Vec<_>>();

    let accepts = variants
        .iter()
        .map(|variant| {
            let variant_ty = &variant.msg_ty;
            quote! {
                if *msg_type_id == std::any::TypeId::of::<#variant_ty>() {
                    return true
                }
            }
        })
        .collect::<Vec<_>>();

    let matches = variants
        .iter()
        .map(|variant| {
            let variant_ident = &variant.enum_ident;
            let variant_ty = &variant.msg_ty;
            quote! {
                Self::#variant_ident(msg) => {
                    zestors::messaging::BoxPayload::new::<#variant_ty>(msg)
                }
            }
        })
        .collect::<Vec<_>>();

    Ok(quote! {
        impl #impl_generics zestors::messaging::Protocol for #ident #ty_generics #where_clause {

            fn try_from_boxed_payload(boxed: zestors::messaging::BoxPayload) -> Result<Self, zestors::messaging::BoxPayload> {
                #(#downcasts)*
                Err(boxed)
            }

            fn accepts_msg(msg_type_id: &std::any::TypeId) -> bool {
                #(#accepts)*
                false
            }

            fn into_boxed_payload(self) -> zestors::messaging::BoxPayload {
                match self {
                    #(#matches)*
                }
            }
        }
    })
}

fn impl_handled_by(item: &ItemEnum, variants: &Vec<ProtocolMsg>) -> Result<TokenStream, Error> {
    let new_generics = {
        let handle_msg_traits: Punctuated<Type, Token![+]> = variants
            .into_iter()
            .map(|variant| {
                let msg_ty = &variant.msg_ty;
                let ty: Type = parse_quote! { zestors::handler::HandleMessage<#msg_ty> };
                ty
            })
            .collect();

        let mut new_generics = item.generics.clone();
        if new_generics.where_clause.is_none() {
            new_generics.where_clause = Some(parse_quote!{ where })
        };
        new_generics.params.push(parse_quote! { H });
        new_generics
            .where_clause
            .as_mut()
            .unwrap()
            .predicates
            .push(parse_quote! {
                H: zestors::handler::Handler + #handle_msg_traits
            });
        new_generics
    };

    let matches: Vec<TokenStream> = variants
        .iter()
        .map(|variant| {
            let variant_ident = &variant.enum_ident;
            let variant_ty = &variant.msg_ty;
            quote! {
                Self::#variant_ident(payload) => {
                    zestors::handler::HandleMessage::<#variant_ty>::handle_msg(
                        handler, state, payload
                    ).await
                }
            }
        })
        .collect();

    let (impl_generics, _, where_clause) = new_generics.split_for_impl();
    let (_, ty_generics, _) = item.generics.split_for_impl();
    let ident = &item.ident;
    Ok(quote! {
        #[zestors::async_trait]
        impl #impl_generics zestors::handler::HandledBy<H> for #ident #ty_generics #where_clause {
            async fn handle_with(
                self,
                handler: &mut H,
                state: &mut H::State,
            ) -> Result<zestors::handler::Flow, H::Exception> {
                match self {
                    #(#matches)*
                }
            }
        }
    })
}

fn modify_protocol_enum(item: &mut ItemEnum) -> Result<Vec<ProtocolMsg>, Error> {
    item.variants
        .iter_mut()
        .map(|variant| {
            let ident = variant.ident.clone();

            if let Fields::Unnamed(fields) = &mut variant.fields {
                if fields.unnamed.len() == 1 {
                    let ty = fields.unnamed.pop().unwrap().value().ty.clone();

                    fields.unnamed.push(Field {
                        attrs: Vec::new(),
                        vis: Visibility::Inherited,
                        ident: None,
                        colon_token: None,
                        ty: parse_quote! {
                            <#ty as zestors::messaging::Message>::Payload
                        },
                    });

                    Ok(ProtocolMsg {
                        enum_ident: ident,
                        msg_ty: ty,
                    })
                } else {
                    Err(Error::new_spanned(fields, "Must have one field"))
                }
            } else {
                Err(Error::new(Span::call_site(), "Must be unnamed enum"))
            }
        })
        .collect()
}
