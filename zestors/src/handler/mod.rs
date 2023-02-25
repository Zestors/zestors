/*!
Implementing an actor is split up in three parts:
1. [`HandlesExit`] -> Define how your actor should handle exiting and receiving a halt-signal
in a correct manner. This trait defines three associated types:
    - `Exit` -> The value that the process exits with.
    - `Inbox` -> The inbox this actor uses to receive messages.
    - `
2. [`Initializable<I>`] -> Define how your actor may be initialized with value `I`.
If `Self: From<I>`, then this trait can be derived with the [`Initializable`] macro.
3. [`HandlesProtocol<P>`] -> This defines how your actor handles messages of the protocol `P`.

Minimal example:
```no_run
// First we create two messages.
// The `SayHello` message..
#[derive(Message, Envelope)]
#[request(u64)]
struct SayHello {
    field_a: u32,
    field_b: String,
}

// .. and the `OtherMessage`.
#[derive(Message, Envelope)]
struct OtherMessage {
    field_a: u32,
}

// Here we can define a protocol with a custom handler-trait generated.
#[protocol(handler = true)]
enum MyProtocol {
    A(SayHello),
    B(OtherMessage),
    C(u32)
}

// Now it's time to define our actor!
// It accepts messages of the `MyProtocol`.
#[derive(Handler, HandleStart)]
#[protocol(MyProtocol)]
struct MyActor {
    field: u32
}

// All that is left is to define our generated handle-trait now.
#[async_trait]
impl HandleMyProtocol for MyActor {
    async fn handle_a(
        &mut self,
        msg: (SayHello, Tx<u64>),
        state: &mut Self::State,
    ) -> Result<Flow, Self::Exception> {
        todo!()
    }

    async fn handle_b(
        &mut self,
        msg: OtherMessage,
        state: &mut Self::State,
    ) -> Result<Flow, Self::Exception> {
        todo!()
    }

    async fn handle_c(
        &mut self,
        msg: u32,
        state: &mut Self::State,
    ) -> Result<Flow, Self::Exception> {
        todo!()
    }
}
```
*/

mod action;
mod core;
mod event_loop;
mod ext;
mod state;
pub use self::core::*;
pub use action::*;
pub use ext::*;
pub use state::*;
