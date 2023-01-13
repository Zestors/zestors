# What is a channel
A [Channel] is that which couples all different aspects of an actor together. It is made up of:
- One unique [Child].
- Zero or more [Addresses](Address)
- One or more processes with their own [Inbox] or [Halter].

The following gives a visual overview of a channel and how it relates to some other concepts:
```other
|¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯|
|                            Channel                          |
|  |¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯|  |¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯|  |
|  |              actor                |  |   Child(Pool)  |  |
|  |  |¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯|  |  |________________|  |
|  |  |         process(es)         |  |                      |
|  |  |  |¯¯¯¯¯¯¯¯¯|  |¯¯¯¯¯¯¯¯¯¯|  |  |                      |
|  |  |  |  tokio  |  |  Inbox/  |  |  |  |¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯¯|  |
|  |  |  |  -task  |  |  Halter  |  |  |  |  Address(es)   |  |
|  |  |  |_________|  |__________|  |  |  |________________|  |
|  |  |_____________________________|  |                      |
|  |___________________________________|                      |
|_____________________________________________________________|
```
### Actor
The term actor is used to refer to all processes of a channel actor together as a unit. In the end it is the actor that accepts messages and sends back replies, which process ends up doing this is merely an implementation detail. In most cases however, an actor consists solely of a single process.

### Process
A process refers to the coupling of a [tokio task](tokio::task) with an [Inbox] or a [Halter]. By passing along this extra object when spawning a process, it allows for graceful shutdown of the process, something which is not possible using standard tokio tasks.

### Inbox/Halter
Every process is either spawned with an [Inbox] or with a [Halter]. Both can be used to receive a halt-signal from an [Address] or [Child] and subsequently clean up the process and exit. An inbox can additionally be used to receive messages sent to it from an [Address].

### Child
A [Child] is a unique reference to an actor, similar to a [tokio JoinHandle](tokio::task::JoinHandle), but different in that it could refer to multiple processes instead of just one. By default, when a child is dropped the processes that the child referred to are shut down; this allows for the creation of supervision trees.

### Address
An [Address] is a clonable reference to an actor and can be used to communicate with the actor. It can be used to halt the actor, and if the actor contains an [Inbox] then messages can also be sent to the actor.

### ActorId
Every actor has a unique [ActorId] generated incrementally when a new actor is spawned.

# Different channels
An actor can use different channels depending on its need. Currently two channels are supported: An [InboxChannel] and a [HalterChannel]. The difference is that an inbox-channel can be used to send messages while a halter-channel can not. Both channels can be transformed into a [dyn DynChannel](DynChannel) which allows different channels to be used as the same type for addresses and children.

### Channel definition
The parameter `C` in a [`Child<_, C, _>`] and an [`Address<C>`](Address) both implement [`DefineChannel`], and define which channel the actor uses. Depending on the parameter `C`, we can send different types of messages to an actor. `C` can either be a sized channel of:
- [`Halter`] : Indicates that the channel is a [HalterChannel] which does not accept any messages.
- [`P where P: Protocol`](Protocol) : Indicates that the channel is an [InboxChannel] which accepts messages that the protocol `P` accepts.

Or a dynamic channel specified by:
- [`Dyn<dyn _>`]/[`Accepts![msg1, ..]`](Accepts!) : Indicates that the channel is a [`dyn DynChannel`](DynChannel) which accepts messages `msg1`, `msg2`, etc.

### Transforming a channel definition
A channel-definition can be transformed into a dynamic channel-definition `C` as long as it implements [`TransformInto<C>`]. 
- A [Halter] channel-definition can be transformed into [`Accepts![]`](Accepts!): A channel which does not accept any methods.
- A [Protocol] channel-definition can be transformed into [`Accepts![msg1, ..]`](Accepts!) where the protocol [accepts](ProtocolFromInto) messages `msg1, ..`.
- A dynamic channel-definition can be transformed into another one where the messages it accepts are a subset of the original messages.

### Downcasting a channel
Any dynamic channel-definition can be downcast into a sized one. For this to be successful, you have to supply the correct channel-definition to the method.