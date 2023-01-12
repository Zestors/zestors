# Accepts
Any [channel_definition](DefinesChannel) can implement [`trait@Accepts<M>`]: This means that the actor accepts messages of type `M`. The channel definition is the parameter `C` in the following structs: [`Child<_, C, _>`], [`Address<C>`] and [`Inbox<C>`]. If this parameter accepts the messages then messages can be sent to that actor.

The following types can implement [`Accepts`](trait@Accepts):
- __Protocol__: Any [`Protocol`] `P` automatically implements [`trait@Accepts<M>`] if it implements [`ProtocolAccepts<M>`].
- __Dynamic__: Any [`Accepts![msg1, ..]`](Accepts!) automatically implements [`Accepts<msg1> + Accepts<..> + ..`](trait@Accepts).

Examples of items to which messages can be sent:
```text
Address<Accepts![u32, MyMessage]>
Address<MyProtocol>
Child<(), Accepts![u32, MyMessage]>
Child<(), MyProtocol>
Inbox<MyProtocol>
```

# Send methods
There are four different basic send methods available:
- `try_send`: Attempts to send a message to the actor. If the inbox is closed/full or if a timeout is returned from the backpressure-mechanic, this method fails.
- `force_send`: Same as `try_send` but ignores any backpressure-mechanic.
- `send`: Attempts to send a message to the actor. If the inbox is full or if a timeout is returned from the backpressure-mechanic, this method will wait until there is space or until the timeout has expired. If the inbox is closed this method fails. 
- `send_blocking`: Same as `send` but blocks the thread for non-async execution environments.

# Unchecked sending
In addition to these send methods, there are unchecked alternatives for any dynamic channels: `try_send_unchecked`, `force_send_unchecked`, `send_unchecked` and `send_blocking_unchecked`. These methods act the same as the regular send methods, but it is checked at runtime that the actor can [accept](trait@Accepts) can accept the message. If the actor does not accept, these method fail and returns the sent message.