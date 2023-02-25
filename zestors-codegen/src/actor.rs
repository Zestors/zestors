use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{
    parse2, parse_quote, Error, Field, Fields, ImplItem, ImplItemMethod, ItemEnum, ItemImpl, Type,
    Visibility,
};

pub fn actor(_attr: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    let item = parse2::<ItemImpl>(item)?;

    // All required items
    let inbox_ty = find_type(&item, "Inbox")?;
    let protocol_ty = find_type(&item, "Protocol")?;
    let exit_method = find_method(&item, "exit")?;

    let ref_ty = ref_type(&item)?;
    let ident = &item.self_ty;
    let start_method = default_start_method();
    let init_method = default_init_method();
    let halt_method = default_halt_method();

    Ok(quote! {
        #[async_trait::async_trait]
        impl Actor for #ident {
            type InboxType = #inbox_ty;
            type Protocol = #protocol_ty;
            type Ref = #ref_ty;
            type Exit = String;
            type Error = BoxError;
            type Init = Self;

            #start_method
            #init_method
            #exit_method
            #halt_method
        }
    })
}

fn ref_type<'a>(item: &'a ItemImpl) -> Result<Type, Error> {
    if let Ok(ty) = find_type(item, "Ref") {
        return Ok(ty.clone());
    } else {
        return Ok(parse_quote! { Address<Self::InboxType> });
    }
}

fn find_type<'a>(item: &'a ItemImpl, ident: &str) -> Result<&'a Type, Error> {
    item.items
        .iter()
        .find_map(|item| match item {
            syn::ImplItem::Type(ty) => {
                if ty.ident.to_string() == ident {
                    Some(&ty.ty)
                } else {
                    None
                }
            }
            _ => None,
        })
        .ok_or(Error::new_spanned(
            item,
            &format!("type {ident} = ..; (not defined)"),
        ))
}

fn find_method<'a>(item: &'a ItemImpl, ident: &str) -> Result<&'a ImplItemMethod, Error> {
    item.items
        .iter()
        .find_map(|item| match item {
            syn::ImplItem::Method(method) => {
                if method.sig.ident.to_string() == ident {
                    Some(method)
                } else {
                    None
                }
            }
            _ => None,
        })
        .ok_or(Error::new_spanned(
            item,
            &format!("fn {ident}(..) {{ .. }} = ...; (not defined)"),
        ))
}

fn default_start_method() -> TokenStream {
    quote! {
        async fn start(address: Address<Self::InboxType>) -> Result<Self::Ref, BoxError> {
            Ok(address)
        }
    }
}

fn default_init_method() -> TokenStream {
    quote! {
        async fn init(init: Self::Init, state: &mut ActorState<Self>) -> Result<Self, Self::Exit> {
            Ok(init)
        }
    }
}

fn default_halt_method() -> TokenStream {
    quote! {
        async fn handle_halt(&mut self, state: &mut ActorState<Self>) -> Result<Flow, Self::Error> {
            state.inbox.close();
            Ok(Flow::ExitNormal)
        }
    }
}
