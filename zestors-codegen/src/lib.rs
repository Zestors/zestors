use proc_macro::TokenStream as TokenStream1;
use quote::quote;

#[proc_macro]
pub fn supervisor(_ts: TokenStream1) -> TokenStream1 {
    quote!().into()
}

/// Derive the `Message` trait.
///
/// ```ignore
/// #[derive(Message)]
/// struct MyCast;
///
/// #[derive(Message)]
/// #[reply(MyReply)]
/// struct MyCall;
/// ```
#[proc_macro_derive(Message, attributes(msg, request))]
pub fn derive_message(item: TokenStream1) -> TokenStream1 {
    message::derive_message(item.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

#[proc_macro_derive(Envelope, attributes(envelope))]
pub fn derive_envelope(item: TokenStream1) -> TokenStream1 {
    envelope::derive_envelope(item.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

#[proc_macro_derive(CustomHandler, attributes(handler))]
pub fn derive_handler(item: TokenStream1) -> TokenStream1 {
    handler::derive_custom_handler(item.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

#[proc_macro_derive(HandleStart, attributes(handler))]
pub fn derive_handle_start(item: TokenStream1) -> TokenStream1 {
    handler::derive_start_handler(item.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

/// Modifies the enum to add receivers and derives `Protocol`/`Accepts<M>` implementations.
///
/// # Usage
/// ```ignore
/// #[protocol]
/// enum MyProtocol {
///     MessageOne(OneOffMsg),
///     MessageTwo(ReplyMsg)
/// }
/// ```
///
/// This creates the following struct:
/// ```ignore
/// enum MyProtocol {
///     MessageOne(OneOffMsg, ()),
///     MessageTwo(ReplyMsg, Tx<MyReply>)
/// }
/// ```
///
/// And also generates the following implementations:
/// ```ignore
/// impl Protocol for MyProtocol { ... }
/// impl Accepts<OneOffMsg> for MyProtocol { ... }
/// impl Accepts<ReplyMsg> for MyProtocol { ... }
/// ```
#[proc_macro_attribute]
pub fn protocol(attr: TokenStream1, item: TokenStream1) -> TokenStream1 {
    protocol::protocol(attr.into(), item.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

#[proc_macro_attribute]
pub fn actor(attr: TokenStream1, item: TokenStream1) -> TokenStream1 {
    actor::actor(attr.into(), item.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

mod message;
mod protocol;
mod envelope;
mod handler;
mod actor;
