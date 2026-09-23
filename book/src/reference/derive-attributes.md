# Derive attributes

`zestors` has four derives, all in `zestors::prelude`:

| Derive | On | Implements |
| --- | --- | --- |
| `Message` | a struct or enum | `Message`: fire-and-forget, or with a reply |
| `Interface` | an enum of `Envelope<M>` variants | `Interface`: the set of messages an actor accepts |
| `HandlerInterface` | the same enum | dispatch from the interface to a `Handler`'s `Handle<M>` impls |
| `StableId` | a message | `StableId`: the message's id on the wire, for [remote messages](../distributed/remote-messages.md) |

Each derive reads its options from `#[msg(...)]` and `#[zestors(...)]`
attributes. The two are interchangeable.

| Key | Used by | Meaning |
| --- | --- | --- |
| `reply = T` | `Message` | The message expects a reply of type `T`. Without it, the message is fire-and-forget. |
| `id = "<uuid>"` | `StableId` | The message's `MessageId`. Required; leave it out and the compile error suggests one. |
| `no_auto_register` | `StableId` | Leave the message out of `ClusterConfig::auto_register`. |
| `interface_path = "path"` | `Message`, `Interface` | Where the generated code finds `zestors-interface`. Default `::zestors::interface`. |
| `actor_path = "path"` | `HandlerInterface` | Where it finds `zestors-actor`. Default `::zestors::actor`. |
| `distr_path = "path"` | `StableId` | Where it finds `zestors-distr`. Default `::zestors::distr`. |

A remote message usually combines both keys in one attribute:

```rust
use serde::{Deserialize, Serialize};
use zestors::interface::Message;
use zestors::prelude::*;

#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = "Vec<u8>", id = "4f9e5a8b-7c6d-4e3f-8a1b-2c3d4e5f6a7b")]
struct Read {
    path: String,
}
```

A few details:

- **Quote reply types that contain `<`:** `reply = "Option<String>"`. A plain
  path such as `reply = u32` needs no quotes.
- **The path keys** are only needed when you depend on the sub-crates directly
  instead of on `zestors`. For example, with `zestors-interface` on its own:
  `#[zestors(interface_path = "zestors_interface")]`.
- **`HandlerInterface`** generates code that names `rootcause::Report`, so the
  crate using it must depend on `rootcause`.
- **`Interface`** only works on enums whose variants each hold exactly one
  `Envelope<M>`, and doesn't support generics. `Message` does support generic
  types.
- **`StableId` on a generic type** is never auto-registered. Register each
  concrete type with `ClusterConfig::register`.
