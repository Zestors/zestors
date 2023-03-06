/*!
# Overview
In order to receive messages an actor must define a [`Protocol`] that specifies which messages it
[`Accepts`]. All messages must in turn define how they are handled by implementing [`Message`].

The [`Message`] trait specifies how a message should be handled by the actor. Since it is unique per
message, every actor is guaranteed to respond in the same way. Normally [`Message`] is derived with the
[`macro@Message`] macro, and most standard rust types like `u32`, `String` and `Vec<String>`, have a
message-implementation.

The [`Protocol`] specifies exactly which messages an actor accepts. Normally the [`Protocol`] trait
can be automatically generated with the [`macro@protocol`] macro.

# The `Message` macro
When using the derive [`macro@Message`] macro it is possible to set a `#[msg(T)]` or `#[request(T)]`
attribute which specifies how the actor should handle the message. There are three types for which this
is implemented automatically:

| Attribute | Result |
|---|---|
| `none` / `#[msg(())]` | A simple message that does not receive a reply, the [`Message::Payload`] is `M` and [`Message::Returned`] is `()`. |
| `#[request(T)]` / `#[msg(Rx<T>)]` | A request of `T` where the [`Message::Payload`] is [`(M, Tx<T>)`](Tx)  and the [`Message::Returned`] is [`Rx<T>`]. |
| `#[msg(Tx<T>)]` | Same as `Rx` but swapped. |

It is possible to create custom types usable in the `#[msg(..)]` attribute by implementing [`MessageDerive<M>`]
for this type.

# Sending
The following send-methods can be used on an [`Address`], [`Child`], [`InboxType`] or any other type that
implements [`ActorRef`] / [`ActorRefExt`].

__Default send methods__: The default methods for sending a message.
- [`try_send`](ActorRefExt::try_send): Attempts to send a message to the actor. If the inbox is closed/full
or if a timeout is returned from the backpressure-mechanic, this method fails.
- [`force_send`](ActorRefExt::force_send): Same as `try_send` but ignores any backpressure-mechanic.
- [`send`](ActorRefExt::send): Attempts to send a message to the actor. If the inbox is full or if a timeout
is returned from the backpressure-mechanic, this method will wait until there is space or until the timeout has
expired. If the inbox is closed this method fails.
- [`send_blocking`](ActorRefExt::send_blocking): Same as `send` but blocks the thread for non-async
execution environments.

__Checked-send methods__: Same as the regular send methods, but instead of checking at
compile-time whether the messages are accepted by the actor, these check it at runtime. These methods are
only valid for [`DynActor`](struct@DynActor) types.
- [`try_send_checked`](ActorRefExt::try_send_checked)
- [`force_send_checked`](ActorRefExt::force_send_checked)
- [`send_checked`](ActorRefExt::send_checked)
- [`send_blocking_checked`](ActorRefExt::send_blocking_checked)

# Requesting
A request is a [`Message`] which expects a reply to be sent from the [`Tx`] to the [`Rx`]. A request can
be manually created with [`new_request`], but is usually automatically created when sending a message.
Requests can be sent in the standard way -- by first sending the request and then waiting for the reply -- but
this can also be done simpler with the following methods:
- [`try_request`](ActorRefExt::try_request)
- [`force_request`](ActorRefExt::force_request)
- [`request`](ActorRefExt::request)

These  methods will send the request and subsequently await a response from the actor with a single method
and `.await` point.

# Envelope
An [`Envelope`](struct@Envelope) is a [`Message`] containing an [`Address`] of where it should be sent. An envelope
can be created with the [`ActorRefExt::envelope`] function.

The [`macro@Envelope`] macro generates a custom trait that allows a user to directly call `.my_message(..)` on an
actor-reference. This custom method constructs an [`Envelope`](struct@Envelope) from the [`Message`] parameters which
can subsequently be sent. This macro is entirely optional and just exists for ergonomics.

| [`actor_reference`](crate::actor_reference) __-->__ |
|---|

# Example
```
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
```
*/

#[allow(unused)]
use crate::all::*;

pub use zestors_codegen::{protocol, Envelope, Message};
mod accepts;
mod box_payload;
mod envelope;
mod errors;
mod message;
mod protocol;
mod request;
pub use accepts::*;
pub use box_payload::*;
pub use envelope::*;
pub use errors::*;
pub use message::*;
pub use protocol::*;
pub use request::*;
