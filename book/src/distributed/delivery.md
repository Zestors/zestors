# Delivery and failure

A local message can only fail because the actor is gone. A remote one crosses a
network, and can fail on the way. This chapter lists what is and isn't
guaranteed.

## At most once

A message is delivered **at most once**. Nothing is ever resent. If no reply
comes, the message may or may not have been handled. Retrying is up to you, so
make messages you retry safe to handle twice.

## Ordering

Messages from one node to one actor arrive in the order they were sent. This
holds per *sending node*. Two nodes messaging the same actor have no order
between their messages.

Each node spreads its traffic to a peer over several independent streams, or
*lanes*. `ClusterConfig::lanes`, 4 by default, sets how many. An actor always
uses the same lane, so its messages stay in order, while a large message to one
actor doesn't hold up messages to actors on other lanes. With one lane,
everything to a peer is in order.

Operations — signals, status reads, monitors — skip the actor's message queue,
as they do locally. A `signal_shutdown` can therefore overtake messages sent
before it.

## When messages are lost

A remote message can disappear without an error at the sender:

- **The receiving actor is overloaded.** A node takes in bursts, but once about
  100 000 messages or 16 MiB are waiting for one actor, it refuses more with
  `RemoteError::Overloaded`. For a call that error comes back as the reply. **A
  cast has no reply, so a refused cast is dropped silently.** The only trace is a
  warning logged on the receiving node. (A local cast waits for room instead.)
- **The connection is down.** While a node reconnects to a peer, messages for
  that peer are dropped. Calls among them fail when their timeout runs out.
- **The node leaves or fails** after the message was sent. Pending calls fail
  with `ClusterReplyError::Disconnected`, and so do monitors.

If a message must not be lost, `call` it, and treat an error as "unknown
outcome".

## Timeouts

Remote calls time out after `ClusterConfig::call_timeout` (30 s) unless the
address or the call sets another timeout. Local calls through a
`ClusterAddress` have no default timeout. Monitors never time out. See
[Addressing and sending](addressing.md#timeouts).

## Which error means what

Sending returns one of three errors:

| Error | Meaning | The message |
| --- | --- | --- |
| `ClusterCastError<M>` | not sent; `reason` is a `CastFailure` | given back in `.msg` |
| `ClusterReplyError` | sent, but no reply came | gone |
| `ClusterCallError<M>` | a `call` failed: `NotSent(ClusterCastError)` or `Reply(ClusterReplyError)` | given back if not sent |

`CastFailure`, the reasons the message never left:

- `NotRunning`: this node hasn't started, or has stopped;
- `NotAMember`: the target node isn't a known member;
- `Unreachable`: the target node can't be connected to right now;
- `Full`: too much is queued for that node (only from `try_cast`);
- `TooLarge`: the encoded message is over 4 MiB;
- `Encode`: encoding failed;
- `Closed`, `NotAccepted`: the actor is on this node, and is closed or doesn't
  accept the message.

`ClusterReplyError`, the reasons the message was sent but no reply came:

- `Remote(RemoteError)`: the other node answered with an error (below);
- `Disconnected`: the node was lost first; outcome unknown;
- `Timeout`: no reply in time; outcome unknown;
- `Decode`: the reply couldn't be decoded.

`RemoteError`, what the other node answered:

- `UnknownMessage`: it doesn't have this message registered;
- `NoSuchActor`: no actor holds that name;
- `NotAccepted`: the actor doesn't accept this message;
- `Closed`: the actor is shutting down;
- `Overloaded`: see above;
- `NoReply`: the actor dropped the request without answering. A local actor
  that drops a request gives the same error;
- `Decode`, `Encode`, `TooLarge`: the message or reply didn't survive the
  wire;
- `Unknown`: a newer node sent an error this version doesn't know.

`ClusterActorOps` methods fail with `ClusterOpError`, which is `NotSent` or
`Reply` in the same way. `Cluster::address` fails with an `AddressError`:
`NoSuchActor`, `TypeMismatch` (the actor doesn't accept the messages), or
`Remote` (the node couldn't be asked).
