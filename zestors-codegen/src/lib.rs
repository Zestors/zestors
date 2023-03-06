use proc_macro::TokenStream as TokenStream1;
#[macro_use]
extern crate quote;

/// Derive the `Message` trait.
///
/// # Usage
/// ```ignore
/// #[derive(Message)]
/// struct MyMessage;
///
/// #[derive(Message)]
/// #[request(..)]
/// struct MyRequest;
/// 
/// // (Same as the one above)
/// #[derive(Message)]
/// #[msg(Rx<..>)]
/// struct MyRequest;
/// ```
/// 
/// This generates the following implementation:
/// ```ignore
/// impl Message for MyMessage { ... }
/// ```
#[proc_macro_derive(Message, attributes(msg, request))]
pub fn derive_message(item: TokenStream1) -> TokenStream1 {
    message::derive_message(item.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}


/// Modifies the enum to implement protocol:
///
/// # Usage
/// ```ignore
/// #[protocol]
/// enum MyProtocol {
///     MessageOne(OneOffMsg),
///     MessageTwo(MessageWithRequest)
/// }
/// ```
///
/// Creates the following enum:
/// ```ignore
/// enum MyProtocol {
///     MessageOne(OneOffMsg),
///     MessageTwo((MessageWithRequest, Tx<..>))
/// }
/// ```
///
/// And also generates the following implementations:
/// ```ignore
/// impl Protocol for MyProtocol { ... }
/// impl Accept<OneOffMsg> for MyProtocol { ... }
/// impl Accept<MessageWithRequest> for MyProtocol { ... }
/// impl<H: Handler> HandledBy<H> for MyProtocol
/// where 
///     H: HandleMessage<OneOffMsg> + HandleMessage<MessageWithRequest>
/// { ... }
/// ```
#[proc_macro_attribute]
pub fn protocol(attr: TokenStream1, item: TokenStream1) -> TokenStream1 {
    protocol::protocol(attr.into(), item.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

/// Generates a trait that can be used to send `Envelope`s of this message.
/// 
/// # Usage
/// ```ignore
/// #[derive(Message, Envelope)]
/// struct MyMessage {
///     arg1: u32,
///     arg2: u64
/// }
/// 
/// #[derive(Message, Envelope)]
/// #[envelope(CustomTraitName, custom_method_name)]
/// struct MyMessage { .. }
/// ```
/// 
/// Which generates the a trait similar to the following:
/// ```ignore
/// trait MyMessageEnvelope {
///     pub fn my_message(&self, arg1: u32, arg2: u64) -> Envelope<..> {
///         ...
///     }
/// }
/// 
/// impl<T, A> MyMessageEnvelope for T 
/// where 
///     T: ActorRef<ActorType = A>,
///     A: Accept<MyMessage>
/// ```
/// 
/// This makes it possible to call `my_message` directly on an `Address` or `Child`.
#[proc_macro_derive(Envelope, attributes(envelope))]
pub fn derive_envelope(item: TokenStream1) -> TokenStream1 {
    envelope::derive_envelope(item.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

/// Implements `HandleExit` for your `Handler`.
/// 
/// # Usage
/// ```ignore
/// #[derive(HandleExit)]
/// struct MyHandler { .. }
/// ```
/// 
/// Which generates the following implementation:
/// ```ignore
/// impl HandleExit for MyHandler {
///     type Exit = ExitReason<MyHandler::Exception>;
/// 
///     async fn handle_exit(
///         self,
///         state: &mut Self::State,
///         reason: ExitReason<Self::Exception>,
///     ) -> ExitFlow<Self> {
///         ExitFlow::Exit(reason)
///     }
/// }
/// ```
#[proc_macro_derive(Handler, attributes(state))]
pub fn derive_handler(item: TokenStream1) -> TokenStream1 {
    handler::derive_handler(item.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

mod message;
mod protocol;
mod envelope;
mod handler;
