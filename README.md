# Under development
This is currently undergoing heavy development, and is not ready for use.

# Zestors

Zestors is a fully-featured actor-framework for tokio. It builds on top of [tiny-actor](https://github.com/jvdwrf/tiny-actor), and adds traits which allow for building more complex systems. The [README](https://github.com/jvdwrf/tiny-actor/blob/main/README.md) of `tiny-actor` describes in detail the basis of the actor-system. The following readme explains what is added on top.

## Addresses
Addresses are divided into two kinds: Normal addresses and dynamic addresses. The difference is in which messages they accept:

### Normal address: `Address<P>`
A normal address is typed by the protocol of the actor. Whichever messages this protocol accepts are the messages that can be sent to the actor.

### Dynamic address: `Address<Dyn<dyn AcceptsX<M1, ..>>> / Address![M1, ...]`
A dynamic address is typed by the messages that the actor can accept, independent of the underlying protocol of the actor. This means that addresses of different protocols can be unified by transforming both of them into the same dynamic address. Examples of a dynamic address's type are: `Address<dyn AcceptsNone>`, `Address<dyn AcceptsTwo<u32, u64>>`.

### Conversion between addresses
When converting to a dynamic address, it's always possible to use the `transform` method. This is statically asserted to be correct and thus fails at compile-time if incorrectly used. If this cannot be statically verified, then the method `try-transform` can be used, which fails at runtime if used incorrectly.

When converting to a normal address, the `downcast` method can be used, which checks if the underlying protocol is the same. This method can fail at runtime if the protocol is not the same.

## Protocol
A protocol defines the messages that an actor can accept by implementing `Accepts<M>`. If a protocol accepts a message `M`, this means that messages of type `M` can be sent to the actor.