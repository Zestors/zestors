<div align="center"><p>
  
[![Crates.io version shield](https://img.shields.io/crates/v/zestors.svg)](https://crates.io/crates/zestors)
[![Docs](https://docs.rs/zestors/badge.svg)](https://crates.io/crates/zestors)
[![Crates.io license shield](https://img.shields.io/crates/l/zestors.svg)](https://crates.io/crates/zestors)

</p></div>


# Zestors
A fast and flexible actor-framework for building fault-tolerant Rust applications, inspired by Erlang/OTP.

## Getting started
Zestors is thoroughly documented at [docs.rs](https://docs.rs/zestors) with guides covering most aspects of the system and a quick-start to get up and running. This is the recommended place to get started.

## Note
Zestors is still early in development, and while the core parts of zestors have been stable for quite some time, from the `handler` module and beyond big changes are expected. Instead of perfecting everything privately, I would rather get it out there to see what people think. Certain parts of the system have been tested extensively but there are (very likely) bugs.

If you have any feedback whether it is a bug, anything unclear or inspiration/ideas then I would appreciate to hear this! 

## Future work
1. Finalize design for the `handler` module and continue work on the (to be released) `supervision` crate.
2. Continue with unit- and integration-tests and build out a few bigger examples.
3. Start work and design for a distributed environment. Everything has been designed from the ground up with distribution in mind, but details have not been worked out.


## Minimal example
```rust
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
```
