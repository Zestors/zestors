/*!
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
#[protocol]
enum MyProtocol {
    A(SayHello),
    B(OtherMessage),
    C(u32)
}

type MyProtocol = Dyn<DynActor!(u32, SayHello, OtherMessage)>;

// Now it's time to define our actor!
// It accepts messages of the `MyProtocol`.
#[derive(HandleInit, HandleExit)]
struct MyHandler {
    field: u32
}

impl Handler for MyHandler {
    type State = Inbox<MyProtocol>;
    type Exception = eyre::Report;
}

#[async_trait]
impl HandleMessage<u32> for MyHandler {
    async fn handle_msg(
        &mut self,
        state: &mut Self::State,
        msg: u32,
    ) -> Result<Flow, Self::Exception> {
        todo!()
    }
}

#[async_trait]
impl HandleMessage<SayHello> for MyHandler {
    async fn handle_msg(
        &mut self,
        state: &mut Self::State,
        msg: (SayHello, Tx<u64>),
    ) -> Result<Flow, Self::Exception> {
        todo!()
    }
}

#[async_trait]
impl HandleMessage<OtherMessage> for MyHandler {
    async fn handle_msg(
        &mut self,
        state: &mut Self::State,
        msg: OtherMessage,
    ) -> Result<Flow, Self::Exception> {
        todo!()
    }
}
```
*/

mod action;
mod traits;
mod state;
pub use self::traits::*;
pub use action::*;
pub use state::*;
