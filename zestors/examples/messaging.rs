#![allow(unused)]
use zestors::prelude::*;
#[macro_use]
extern crate zestors;

// First we will define two different messages.
// A simple message ..
#[derive(Message, Envelope, Debug, Clone)]
struct MyMessage {
    param1: String,
    param2: u32,
}

// .. and a request.
#[derive(Message, Envelope, Debug, Clone)]
#[request(u32)]
struct MyRequest {
    param1: String,
    param2: u32,
}

// Now we are ready to define our protocol!
// This protocol accepts three messages: `MyMessage`, `MyRequest` and `u32`.
#[protocol]
enum MyProtocol {
    Msg1(MyMessage),
    Msg2(MyRequest),
    U32(u32),
}
// That is our actor-definition done.

// We can now start using it!
#[tokio::main]
async fn main() {
    // Let's spawn a basic actor that just prints any messages it receives ..
    let (child, address) = spawn(|mut inbox: Inbox<MyProtocol>| async move {
        loop {
            match inbox.recv().await.unwrap() {
                MyProtocol::Msg1(msg1) => {
                    println!("Received: {:?}", msg1);
                }
                MyProtocol::Msg2((msg2, tx)) => {
                    println!("Received: {:?}", msg2);
                    tx.send(msg2.param2 + 10).unwrap();
                }
                MyProtocol::U32(msg_u32) => {
                    println!("Received: {:?}", msg_u32);
                }
            }
        }
    });

    let my_msg = MyMessage {
        param1: "hi".to_string(),
        param2: 10,
    };
    let my_request = MyRequest {
        param1: "hi".to_string(),
        param2: 10,
    };

    // .. and send it some messages!
    address.send(10u32).await.unwrap();
    address.send(my_msg.clone()).await.unwrap();

    // We can also request (with boilerplate) ..
    let reply: Rx<u32> = address.send(my_request.clone()).await.unwrap();
    assert_eq!(20, reply.await.unwrap());

    //  .. or do it without the boilerplate!
    assert_eq!(20, address.request(my_request.clone()).await.unwrap());

    // It is also possible to send a message by creating an envelope and sending that ..
    address.envelope(my_msg.clone()).send().await.unwrap();
    address
        .envelope(my_request.clone())
        .request()
        .await
        .unwrap();

    // .. or directly by using our derived Envelope trait!
    address
        .my_message("hi".to_string(), 10)
        .send()
        .await
        .unwrap();
    address
        .my_request("hi".to_string(), 10)
        .request()
        .await
        .unwrap();
}
