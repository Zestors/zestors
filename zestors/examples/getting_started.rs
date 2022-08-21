#[macro_use]
extern crate zestors;

use std::time::Duration;
use zestors::{
    actor::{spawn, Addr, Inbox},
    actor_type::{Accepts, IntoAddress},
    config::Config,
    error::{ExitError, RecvError},
    request::{Request, Rx},
};

//-------------------------------------------------
//  Step 1: Define the messages you will use
//-------------------------------------------------

// This message will be a simple message, without any reply.
#[derive(Message, Debug)]
struct MyMessage {
    number: u32,
}

// Now we define a message which expects a response of a `String`.
#[derive(Message, Debug)]
#[msg(Request<String>)]
struct Echo {
    string: String,
}

//-------------------------------------------------
//  Step 2: Define the protocol of the actor
//-------------------------------------------------

// Here we define our protocol as an enum.
// It accepts 3 messages: `Echo`, `MyMessage` and `i64`.
//
// This macro modifies the enum such that a response can be sent back, this
// will be made obvious later.
#[derive(Debug)]
#[protocol]
enum MyProtocol {
    Echo(Echo),
    MyMessage(MyMessage),
    Number(i64),
}

#[tokio::main]
async fn main() {
    //-------------------------------------------------
    //  Step 3: Spawn the actor
    //-------------------------------------------------

    // Here, we spawn the actor with a default configuration.
    //
    // We get back a `Child<String, MyProtocol>` and a `Address<MyProtocol>`.
    let (mut child, address) = spawn(
        Config::default(),
        |mut inbox: Inbox<MyProtocol>| async move {
            loop {
                match inbox.recv().await {
                    Ok(msg) => match msg {
                        // Here we receive the echo message, which wants back a reply!
                        // This reply can be sent back to the `Tx`, and was automatically
                        // created for us.
                        MyProtocol::Echo((Echo { string }, tx)) => {
                            println!("Echoing string: {}", string);
                            let _ = tx.send(string);
                        }
                        // The MyMessage does not have this `Tx`, as can be seen.
                        MyProtocol::MyMessage(MyMessage { number }) => {
                            println!("Received number: {}", number);
                        }
                        // And neither does the `u32`.
                        MyProtocol::Number(number) => {
                            println!("Received number: {}", number);
                        }
                    },
                    Err(e) => match e {
                        // Here we received a halt-signal, so we should exit now.
                        RecvError::Halted => break "Halted",
                        // And if the inbox is closed and empty, we should also exit.
                        RecvError::ClosedAndEmpty => break "ClosedAndEmpty",
                    },
                }
            }
        },
    );

    //-------------------------------------------------
    //  Step 4: Send messages!
    //-------------------------------------------------

    // The following messages don't get a reply:
    let _: () = address.send(10 as i64).await.unwrap();
    let _: () = address.send(MyMessage { number: 11 }).await.unwrap();

    // But our echo will get back a reply!
    let rx: Rx<String> = address
        .send(Echo {
            string: "Hi there".to_string(),
        })
        .await
        .unwrap();

    // We can await the `Tx` to get it back:
    let reply = rx.await.unwrap();
    assert_eq!(reply, "Hi there".to_string());

    //-------------------------------------------------
    //  (Step 4.1): Conversion of addresses
    //-------------------------------------------------

    // (Just skip to Step 5 if the readme is getting to long ;))

    // Now you might be wondering, what is the use of all these complicated traits?
    // Well, we can convert our `Address<MyProtocol>` into an `Address![i64, Echo]`.
    // This conversion is checked at compile-time, so it is impossible for errors to
    // occur at runtime.

    let address: DynAddress![i64, Echo] = address.into_dyn();
    address.send(10 as i64).await.unwrap();

    // This address can be transformed further:

    let address: DynAddress![Echo] = address.transform();
    let _tx = address
        .send(Echo {
            string: "Hi".to_string(),
        })
        .await
        .unwrap();

    // And it can finally be converted back into our original address:

    let address: Addr<MyProtocol> = address.downcast().unwrap();
    address.send(10 as i64).await.unwrap();

    //-------------------------------------------------
    //  (Step 4.2): More cool things
    //-------------------------------------------------

    // What is so great about this?
    //
    // We can now transform addresses with different types into the same ones.
    // As long as an address accepts the message `u32`, it can be transformed into
    // an `Address![u32]`. 2 examples of this usage:

    async fn accepts_u32_v1(address: DynAddress![i64, Echo]) {
        address.send(10 as i64).await.unwrap();
    }

    async fn accepts_u32_v2<T>(address: T)
    where
        T: IntoAddress<DynAccepts![i64, Echo]>,
    {
        let address: DynAddress![i64, Echo] = address.into_address();
        address.send(10 as i64).await.unwrap();
    }

    async fn accepts_u32_v3<T>(address: Addr<T>)
    where
        T: Accepts<i64> + Accepts<Echo>,
    {
        address.send(10 as i64).await.unwrap();
    }

    // v1 must be used with a dynamic address
    accepts_u32_v1(address.clone().into_dyn()).await;

    // v2 can also be used with a static address
    accepts_u32_v2(address.clone()).await;
    accepts_u32_v2(address.clone().into_dyn::<DynAccepts![Echo, i64]>()).await;

    // Just like v3
    accepts_u32_v3(address.clone()).await;
    accepts_u32_v3(address.clone().into_dyn::<DynAccepts![Echo, i64]>()).await;

    // All of these functions can be used with addresses of different protocols. This
    // principle allows for building generic solutions that work for different types of
    // addresses.

    // This is done without any overhead for normal message sending. Messages are NOT
    // boxed before being sent, but the normal `Protocol` is sent through the channel.

    //-------------------------------------------------
    //  Step 5: Shutting down the actor
    //-------------------------------------------------

    // Now we would like our actor to shutdown gracefully again. Luckily there is builtin
    // functionality for this.

    // Since we used a default config, we could drop the `Child`, which would halt and abort
    // the actor, and cause for a graceful exit. But there is an even better way to do this:

    // We will give the child 1 second to shut down before we try to abort it.
    match child.shutdown(Duration::from_secs(1)).await {
        Ok(exit) => {
            // Now, it should have exited with the string "Halted".
            assert_eq!(exit, "Halted");
        }
        Err(error) => match error {
            ExitError::Panic(_) => panic!("Actor exited because of a panic"),
            ExitError::Abort => panic!("Actor exited because it was aborted"),
        },
    }
}
