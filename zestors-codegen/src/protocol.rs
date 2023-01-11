use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{parse2, parse_quote, Error, Field, Fields, ItemEnum, Type, Visibility};

pub fn protocol(_attr: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    let mut item = parse2::<ItemEnum>(item)?;

    let variants = extend_enum(&mut item)?;
    let impl_protocol = impl_protocol(&item, &variants)?;
    let impl_accepts = impl_accepts(&item, &variants)?;

    Ok(quote! {
        #item
        #impl_protocol
        #impl_accepts
    })
}

struct ProtocolVariant {
    ident: Ident,
    ty: Type,
}

fn impl_accepts(item: &ItemEnum, variants: &Vec<ProtocolVariant>) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    let accepts = variants.iter().map(|variant| {
            let variant_ty = &variant.ty;
            let variant_ident = &variant.ident;
            quote! {
                impl #impl_generics zestors::actor_type::ProtocolAccepts<#variant_ty> for #ident #ty_generics #where_clause {
                    fn from_msg(
                        msg: zestors::actor_type::Sent<#variant_ty>
                    ) -> Self {
                        Self::#variant_ident(msg)
                    }

                    fn try_into_msg(self) -> Result<
                        zestors::actor_type::Sent<#variant_ty>,
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

fn impl_protocol(item: &ItemEnum, variants: &Vec<ProtocolVariant>) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    let downcasts = variants
        .iter()
        .map(|variant| {
            let variant_ident = &variant.ident;
            let variant_ty = &variant.ty;
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
            let variant_ty = &variant.ty;
            quote! {
                if *msg_type_id == core::any::TypeId::of::<#variant_ty>() {
                    return true
                }
            }
        })
        .collect::<Vec<_>>();

    let matches = variants
        .iter()
        .map(|variant| {
            let variant_ident = &variant.ident;
            let variant_ty = &variant.ty;
            quote! {
                Self::#variant_ident(msg) => {
                    zestors::actor_type::BoxedMessage::new::<#variant_ty>(msg)
                }
            }
        })
        .collect::<Vec<_>>();

    Ok(quote! {
        impl #impl_generics zestors::actor_type::Protocol for #ident #ty_generics #where_clause {

            fn try_from_box(boxed: zestors::actor_type::BoxedMessage) -> Result<Self, zestors::actor_type::BoxedMessage> {
                #(#downcasts)*
                Err(boxed)
            }

            fn accepts_msg(msg_type_id: &core::any::TypeId) -> bool {
                #(#accepts)*
                false
            }

            fn into_box(self) -> zestors::actor_type::BoxedMessage {
                match self {
                    #(#matches)*
                }
            }
        }
    })
}

fn extend_enum(item: &mut ItemEnum) -> Result<Vec<ProtocolVariant>, Error> {
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
                            zestors::actor_type::Sent<#ty>
                        },
                    });

                    Ok(ProtocolVariant { ident, ty })
                } else {
                    Err(Error::new_spanned(fields, "Must have one field"))
                }
            } else {
                Err(Error::new(Span::call_site(), "Must be unnamed enum"))
            }
        })
        .collect()
}
