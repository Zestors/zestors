#[macro_use]
extern crate zestors;
use zestors::{messaging::RecvError, prelude::*};

// Let's define a single request ..
#[derive(Message, Envelope, Debug)]
#[request(u32)]
struct MyRequest {
    param: String,
}

// .. and create a protocol that accepts this request.
#[protocol]
enum MyProtocol {
    MyRequest(MyRequest),
    String(String),
}

#[tokio::main]
async fn main() {
    // Now we can spawn a simple actor ..
    let (child, address) = spawn(|mut inbox: Inbox<MyProtocol>| async move {
        loop {
            match inbox.recv().await {
                Ok(msg) => match msg {
                    MyProtocol::MyRequest((request, tx)) => {
                        println!("Received request: {:?}", request.param);
                        tx.send(100).unwrap();
                    }
                    MyProtocol::String(string) => {
                        println!("Received message: {:?}", string);
                    }
                },
                Err(e) => match e {
                    RecvError::Halted => break "Halted",
                    RecvError::ClosedAndEmpty => break "Closed",
                },
            }
        }
    });

    // .. and send it some messages!
    address.send("Hi".to_string()).await.unwrap();

    let response = address
        .request(MyRequest {
            param: "Hi".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(response, 100);

    let response = address
        .my_request("Hi".to_string())
        .request()
        .await
        .unwrap();
    assert_eq!(response, 100);

    child.halt();
    assert_eq!(child.await.unwrap(), "Halted");
}
