Every actor must have a [Protocol] that defines exactly which messages it can receive and how it responds to those messages. 

# Messages
Every message sent in zestors has to implement the [Message] trait. This trait specifies how the actor should handle the message, i.e. should it send back a reply and if so what type should this reply be of. By defining this on the message, it allows a message to be sent to different actors while always receiving a reply of the same kind.

Normally, the [Message] trait is not implemented manually, but is instead derived with the [`#[derive(Message)]`](macro@Message) macro. Most standard rust types like `u32`, `String` or `Vec<String>` have this automatically implemented.

# Protocol
Every actor must have one [Protocol] which defines exactly which messages it can receive using the trait [`ProtocolFrom<Message>`]. Normally, the [Protocol] and [ProtocolFrom] traits are not implemented manually but can be derived using the [macro@protocol] macro.

# Example defining a custom protocol

```rust
use zestors::core::{Message, Rx, protocol};

// This message does not require a reply.
#[derive(Message)]
struct MyMessage1(String);

// While this message requires a reply of `u32`.
#[derive(Message)]
#[msg(Rx<u32>)]
struct MyMessage2(String);

// Now we have a protocol that can accepts three different messages.
#[protocol]
enum MyProtocol {
    Msg1(MyMessage1),
    Msg2(MyMessage2),
    U32(u32)
}
```