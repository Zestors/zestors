use std::time::Duration;

use zestors::{
    action, async_trait,
    channel::inbox::Inbox,
    handler::{Action, ExitReason, Flow, HandleMessage, Handler, HandlerExt},
    messaging::Tx,
    monitoring::ActorRefExt,
    protocol, Envelope, Message,
};
use zestors_codegen::HandleExit;

#[derive(Message, Envelope, Debug)]
#[request(u32)]
pub struct GetAndPrint {
    val_a: u32,
    val_b: String,
}

#[protocol]
#[derive(Debug)]
pub enum MyProtocol {
    A(u32),
    B(GetAndPrint),
    C(Action<MyHandler>),
}

#[derive(HandleExit)]
pub struct MyHandler {
    pub handled: u32,
}

impl Handler for MyHandler {
    type Exception = anyhow::Error;
    type State = Inbox<MyProtocol>;
}

#[async_trait]
impl HandleMessage<u32> for MyHandler {
    async fn handle_msg(
        &mut self,
        _state: &mut Self::State,
        msg: u32,
    ) -> Result<Flow, Self::Exception> {
        self.handled += msg;
        Ok(Flow::Continue)
    }
}

#[async_trait]
impl HandleMessage<GetAndPrint> for MyHandler {
    async fn handle_msg(
        &mut self,
        _state: &mut Self::State,
        (msg, tx): (GetAndPrint, Tx<u32>),
    ) -> Result<Flow, Self::Exception> {
        self.handled += msg.val_a;
        println!("{}", msg.val_b);
        let _ = tx.send(self.handled);
        Ok(Flow::Continue)
    }
}

#[tokio::main]
async fn main() {
    let (child, address) = MyHandler { handled: 0 }.spawn();
    address.send(10u32).await.unwrap();

    for i in 0..10 {
        let response = address
            .get_and_print(i, String::from("Printing a message"))
            .request()
            .await
            .unwrap();
        println!("Got response {i} = {response}");
    }

    child
        .send(action!(|handler: &mut MyHandler, _state| async move {
            println!("I will print this message as well: {}", handler.handled);
            Ok(Flow::Continue)
        }))
        .await
        .unwrap();

    child.halt();
    let exit = child.await;
    println!("{exit:?}");
}
