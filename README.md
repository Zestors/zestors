[![Crates.io](https://img.shields.io/crates/v/zestors)](https://crates.io/crates/zestors)
[![Documentation](https://docs.rs/zestors/badge.svg)](https://docs.rs/zestors)

# Zestors
Zestors is a message-centric actor framework for tokio. It builds on top of [tiny-actor](https://github.com/jvdwrf/tiny-actor) while adding a layer of abstraction that allows for building more complex systems. 

**Fast**: There is 0 overhead when using `zestors` over `tiny-actor`, performace should be very similar to spawning a task with channel while gaining a lot of features. Messages are not boxed before sending but have exactly the same layout as specified in the protocol.

**Flexible**: Addresses can be typed either by their protocol *(static)* or by the messages that it accepts *(dynamic)*. As far as I know, this is the only rust actor-framework to allow this, and is very essential for easy message-sending.

**Extensible**: Wherever possible, features have been implemented as external libraries. This means that it is possible to use your own custom message-types, distributed framework, etc.

# Getting started
To get started it is best to read the [docs](https://docs.rs/zestors/0.0.1/zestors/) of zestors first. For more information about the behaviour of supervision, spawning, aborting, etc, the [README](https://github.com/jvdwrf/tiny-actor/blob/main/README.md) of `tiny-actor` has an in-depth explanation of these concepts.

*Please note that zestors is still in early development and could (will) contain bugs*

# Repository organization
The main crate for users is `zestors`, which exports other crates within this repository. When building libraries, it's better to depend only on those crates actually necessary, since these will be more stable.

# Contributing
It would be amazing for other people to start contributing to zestors. I'm currently developing zestors in my spare time while balancing it with a study, so any help is appreciated. :)

I have tried to keep everything as modular as possible, so that a lot of things can be built as external libraries. For an example library, look at `zestors-request`, which implements messages with a reply as an external crate. Some libraries which I think would be a great additions are:
- `Actor`: An actor trait which allows handing messages similar to [Actix](https://docs.rs/actix/0.13.0/actix/) or the Erlang/Elixir genserver. For some inspiration you can take look at `zestors-actor`, where I started on such a solution. Eventlually I would like to integrate a version of this trait into `zestors`, but currently I'm a bit unsure about the best way to do this.
- `Supervision`: I imagine this would be very similar to Erlang/Elixir's OTP framework. It's possible to go many different directions with this, but the basics would be the ability to automatically supervise/shutdown actors. What would also be very nice to have is a way to inspect supervision-trees at runtime, though I'm not entirely sure what the best way is for this to be done. (This might only be possible if combined with the actor-trait)
- `Registry`: The ability to easily register processes in a (global) registry.
- `Distribution`: The ability to send messages across different binaries/physical computers. I will be working on implementing this in `zestors-distributed`.
- Anything else you come up with ;)