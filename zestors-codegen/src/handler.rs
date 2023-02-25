use heck::ToSnakeCase;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    parse::Parse, parse2, parse_quote, punctuated::Punctuated, spanned::Spanned, token::Colon,
    Attribute, Error, Field, Fields, FnArg, Generics, Item, ItemEnum, Meta, Pat, PatIdent, PatType,
    Token,
};

pub fn derive_custom_handler(item: TokenStream) -> Result<TokenStream, Error> {
    let _item = parse2::<ItemEnum>(item)?;

    Ok(quote! {
        #[async_trait::async_trait]
        trait HandleMyProtocol: HandleProtocol
        where
            Self::Inbox: ActorInbox<Protocol = MyProtocol>
        {
            async fn handle_a(
                &mut self,
                msg: u32,
                state: &mut DefaultState<Self>,
            ) -> Result<Flow<Self>, BoxError>;

            async fn handle_b(
                &mut self,
                msg: (MyMessage, Tx<&'static str>),
                state: &mut DefaultState<Self>,
            ) -> Result<Flow<Self>, BoxError>;
        }

        #[async_trait::async_trait]
        impl<T> HandleProtocol for T
        where
            T: HandleMyProtocol,
            T::Inbox: ActorInbox<Protocol = MyProtocol>
        {
            async fn handle_protocol(
                &mut self,
                msg: <Self::Inbox as ActorInbox>::Protocol,
                state: &mut DefaultState<Self>,
            ) -> Result<Flow<T>, BoxError> {
                match msg {
                    MyProtocol::A(msg) => self.handle_a(msg, state).await,
                    MyProtocol::B(msg) => self.handle_b(msg, state).await,
                }
            }
        }
    })
}

pub fn derive_start_handler(_item: TokenStream) -> Result<TokenStream, Error> {
    Ok(quote! {
        #[async_trait]
        impl HandleStart<Self> for MyActor {
            type Ref = Address<Self::Inbox>;

            async fn on_spawn(address: Address<Self::Inbox>) -> Result<Self::Ref, BoxError> {
                Ok(address)
            }

            async fn handle_start(
                init: Self,
                state: &mut DefaultState<Self>,
            ) -> Result<Self, Self::Exit> {
                Ok(init)
            }
        }
    })
}
