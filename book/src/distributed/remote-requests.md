# Replies inside messages

A message's reply type is sent back to the sender automatically. Sometimes the
reply channel has to be part of the message itself: the actor forwards the job
to another actor, which answers directly, or the message is a `cast` whose
answer comes later.

Locally that channel is a `Request<T>`. Across the network it is a
`RemoteRequest<T>`:

```rust
use serde::{Deserialize, Serialize};
use zestors::interface::Message;
use zestors::prelude::*;

#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(id = "7c6f8a0e-2b1d-4e3f-9a8b-0c1d2e3f4a5b")]
struct CountLetters {
    text: String,
    reply: RemoteRequest<usize>,
}

# #[tokio::main]
# async fn main() {
// Keep the `Reply`, send the `RemoteRequest` in the message.
let (request, reply) = RemoteRequest::new();
let msg = CountLetters { text: "world".into(), reply: request };

// ...the actor that receives `msg`, on whichever node, answers it:
let CountLetters { text, reply: request } = msg;
request.reply(text.chars().count()).unwrap();

assert_eq!(reply.await.unwrap(), 5);
# }
```

When the message goes to another node, the request stays on the sending node,
and the receiving actor gets a stand-in. Whatever the actor answers is sent back
and resolves the original. Locally it is just a `Request`.

- There is **no timeout**. Like a local request, the `Reply` waits until it is
  answered, or fails when the request is dropped or the node it went to is lost.
- If the message is never sent, the request is dropped, and the `Reply` fails.
- A `RemoteRequest` only works inside a message. It can't be serialized any
  other way, and can't be part of a reply.
- `T` has to be encodable, like any message.
