# Operating on remote actors

`ClusterActorOps` (in the prelude) is the counterpart of `ActorOps` for a
`ClusterAddress`. Everything that doesn't send a message lives here:

| Methods | What they do |
| --- | --- |
| `signal_shutdown`, `signal_suspend`, `signal_resume`, `signal` | send a signal |
| `ping` | check that the actor is alive and has caught up with its signals |
| `status`, `msg_len`, `signal_len`, `reached_backpressure`, `is_dead`, … | read one value |
| `info`, `snapshot` | read everything at one instant |
| `members`, `accepts`, `is_superset_of` | ask which remote messages it accepts |
| `monitor_any`, `monitor_exit`, `monitor_init`, `monitor_running`, `monitor_accepts_messages` | wait for a status |

For an actor on another node, each of these is a round trip. So they are all
async, and all can fail with a `ClusterOpError`: the request couldn't be sent,
or no answer came. They are not queued behind the actor's messages, so they
answer even when the actor has a backlog.

## Monitoring

The `monitor_*` family works as it does locally, and waits across nodes. Their
results are nested:

- the outer `Result` is about the network;
- the inner one is what the actor did.

For example, `monitor_exit()` gives `Ok(Ok(()))` for a normal exit and
`Ok(Err(e))` for a failed one.

```rust
# use serde::{Deserialize, Serialize};
# use zestors::{
#     distr::{ClusterReplyError, ClusterOpError, sim::SimNetwork},
#     interface::{Envelope, Interface, Message},
#     prelude::*,
#     runtime::spawn,
#     supervisor::Supervisor,
# };
# #[derive(Message, StableId, Serialize, Deserialize, Debug)]
# #[msg(id = "b5a4c0de-0000-4000-8000-000000000002")]
# struct Work;
# #[derive(Interface, Debug)]
# enum WorkerInterface { Work(Envelope<Work>) }
# #[tokio::main(flavor = "current_thread", start_paused = true)]
# async fn main() {
# let net = SimNetwork::new(1);
# let a = ClusterNode::new(Supervisor::blueprint().rand_name(),
#     ClusterConfig::new("node-a", net.backend("10.0.0.1:7000")));
# let b = ClusterNode::new(Supervisor::blueprint().rand_name(),
#     ClusterConfig::new("node-b", net.backend("10.0.0.2:7000"))
#         .seed(Seed::new("node-a", "10.0.0.1:7000"))
#         .register::<Work>());
# let _worker = spawn(Name::new_static("worker"), |mut inbox: Inbox<WorkerInterface>| async move {
#     while inbox.recv().await.is_some() {}
#     Ok(())
# }).unwrap();
# let cluster = a.cluster();
# tokio::spawn(a.run());
# tokio::spawn(b.run());
# cluster.wait_for_members(1).await;
// On node-a, for an actor on node-b:
let worker = cluster
    .address::<WorkerInterface>(GlobalName::new("worker", "node-b"))
    .await
    .unwrap();

let exited = tokio::spawn({
    let worker = worker.clone();
    async move { worker.monitor_exit().await }
});

worker.signal_shutdown().await.unwrap();

match exited.await.unwrap() {
    Ok(Ok(())) => println!("exited normally"),
    Ok(Err(error)) => println!("exited with an error: {error}"),
    // The actor's node was lost: there is no telling what happened to it.
    Err(ClusterOpError::Reply(ClusterReplyError::Disconnected)) => println!("node lost"),
    Err(other) => println!("couldn't monitor: {other}"),
}
# }
```

A monitor on another node:

- is held by that node until the status is reached. It has **no timeout**, not
  even the call timeout, so it can wait for as long as the actor lives;
- ends with `ClusterReplyError::Disconnected` if the connection to that node is
  lost, or the node leaves or fails. This is Erlang's `noconnection`: the actor
  may still be running;
- is cancelled on the other node when you drop the future;
- is cleaned up by the other node if *your* node goes away.

`monitor_init` differs slightly from the local one: an actor whose name is
reserved but has never been spawned counts as `Exited`.

## Supervising across nodes

A supervisor supervises actors in its own process only. To react to an actor on
another node, monitor it, for example from a `Handler`'s `next_event`, and
decide there what to do when it exits or its node disconnects.
