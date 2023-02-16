/*!
# Defining a protocol
In order to send messages, actors must define a [`Protocol`] which specifies the messages it
accepts. All messages must implement [`Message`] which specifies how an actor should handle
that message. Therefore, when defining a protocol, one must first define the messages before
the protocol itself.

The trait [`Message`] specifies how a message should be handled by the actor. Since this is unique for
every message, different actors are guaranteed to respond in the same way to the same messages.
Normally [`Message`] can be derived with the [`macro@Message`] macro and most standard rust types,
like `u32`, `String` and `Vec<String>`, have an auto-implementation.

A [`Protocol`] specifies exactly which messages an actor accepts. Normally the [`Protocol`] trait
can be created using the [`macro@protocol`] macro.

# Different message-kinds
When using the derive [`macro@Message`] macro it is possible to give this a `#[msg(T)]` attribute to
specify how the actor should handle the message. By default, it is possible to give three types to
this macro:
- `()` -> The [`Message::Payload`] is exactly the same as the message itself. (This is the default)
- [`Rx<T>`] -> The message's [`Message::Payload`] is `(M, Tx<T>)` and the [`Message::Returned`] is
an `Rx<T>`. This means that the actor will send back a reply through the [`Tx`].
- [`Tx<T>`] -> Same as with `Rx<T>`, but swapped around. This allows for sending another message at a later
point in time.

It is possible to create custom types for this position by implementing [`MessageDerive<M>`] for this
type.

# Sending
There are four different basic send methods available:
- `try_send`: Attempts to send a message to the actor. If the inbox is closed/full or if a timeout
is returned from the backpressure-mechanic, this method fails.
- `force_send`: Same as `try_send` but ignores any backpressure-mechanic.
- `send`: Attempts to send a message to the actor. If the inbox is full or if a timeout is returned
from the backpressure-mechanic, this method will wait until there is space or until the timeout has
expired. If the inbox is closed this method fails.
- `send_blocking`: Same as `send` but blocks the thread for non-async execution environments.

In addition to these send methods, there are checked alternatives for any dynamic channels:
`try_send_checked`, `force_send_checked`, `send_checked` and `send_blocking_checked`. These
methods act the same as the regular send methods, but it is checked at runtime that the actor can
[accept](Accept) the message. If the actor does not accept, these method fail.

| [`monitoring`] __-->__ |
|---|

# Example
```
use zestors::*;

// First we will define two different messages //

// This message does not require a reply.
#[derive(Message, Debug)]
struct MyMessage1(String);

// While this message requires a reply of `u32`.
#[derive(Message, Debug)]
#[msg(Rx<u32>)]
struct MyMessage2(String);

// After which we can now define our protocol //

// This protocol accepts three messages: `MyMessage1`, `MyMessage2` and `u32`
#[protocol]
enum MyProtocol {
    Msg1(MyMessage1),
    Msg2(MyMessage2),
    U32(u32)
}

# tokio_test::block_on(main());
async fn main() {
    // It is now possible to spawn an actor with this specific inbox:
    let (child, address) = spawn(|mut inbox: Inbox<MyProtocol>| async move {
        loop {
            match inbox.recv().await.unwrap() {
                MyProtocol::Msg1(msg1) => {
                    println!("Received: {:?}", msg1);
                }
                MyProtocol::Msg2((msg2, tx)) => {
                    println!("Received: {:?}", msg2);
                    tx.send(20u32).unwrap();
                }
                MyProtocol::U32(msg_u32) => {
                    println!("Received: {:?}", msg_u32);
                }
            }
        }
    });

    // And we can send it messages
    address.send(10u32).await.unwrap();
    address.send(MyMessage1("hello".to_string())).await.unwrap();

    // Since message 2 required a reply, we can now see this in action
    let reply: Rx<u32> = address.send(MyMessage2("hello".to_string()))
        .await
        .unwrap();
    assert_eq!(20, reply.await.unwrap());

    // Or the same thing but shorter
    assert_eq!(
        20,
        address.send(MyMessage2("hello".to_string())).into_recv().await.unwrap()
    );
}
```
*/

#[allow(unused)]
use crate::*;

pub use zestors_codegen::{protocol, Message};
mod any_payload;
mod errors;
mod into_recv;
mod message;
mod protocol;
pub use any_payload::*;
pub use errors::*;
pub use into_recv::*;
pub use message::*;
pub use protocol::*;
pub mod request;
