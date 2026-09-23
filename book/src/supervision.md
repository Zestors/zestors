# Supervision

Supervision is optional. Everything so far works without it. It adds
Erlang/OTP-style restart trees: a supervisor starts a set of children, notices
when one exits, and restarts it according to a policy.

## The pieces

- A **`Blueprint`** is a recipe for creating an actor, so that it can be created
  again after a crash. Any `Actor` that is `Clone + Debug` is its own blueprint.
  `fn_blueprint(|| …)` builds one from a closure, and `fn_actor` / `fn_task` turn
  closures into actors.
- A **`ChildSpec`** pairs a blueprint with the `Name` it runs under and a
  `ChildConfig`. `blueprint.name("worker")?` reserves the name right away;
  `blueprint.rand_name()` makes one up.
- A **`ChildConfig`** says what a supervisor does with the child:
  - `restart_mode`: `Always`, `OnError` (the default: only after an error,
    panic or abort) or `Never`;
  - `intensity`: an optional per-child restart budget;
  - `init_timeout`, `abort_timeout`, `start_timeout`.

  Set these with `with_mode`, `with_abort_timeout`, and so on.
- A **`Supervisor`** is an ordinary actor, built from
  `Supervisor::blueprint()`.
- A **`SupervisionStrategy`** decides what gets restarted when a child exits:

  | Strategy | Restarts |
  | --- | --- |
  | `OneForOne` (default) | only the child that exited |
  | `OneForAll` | every child |
  | `RestForOne` | the child that exited, and every child started after it |

- A **`RestartIntensity`**, for example `RestartIntensity::new(5,
  Duration::from_secs(10))`, is the most restarts allowed in a window. A
  supervisor has one (by default 3 restarts in 5 minutes, set with
  `.intensity(…)`), and a child can have its own as well. When either runs out,
  the supervisor stops its children and exits itself, and its own supervisor
  takes over.

## Running a tree as a program

`Node` runs a root supervisor as the whole program. It starts the supervisor,
shuts it down gracefully on Ctrl+C or SIGTERM, and forces an exit on a second
signal. It returns when the root supervisor exits, and never restarts it:
restarting the whole program is the job of whatever runs it (systemd,
Kubernetes, …).

```rust
use zestors::interface::{Envelope, Interface, Message};
use zestors::prelude::*;
use zestors::supervision::messages::GetChildren;
use zestors::supervisor::{Node, Supervisor};

#[derive(Message, Debug)]
struct Ping;

#[derive(Interface, HandlerInterface, Debug)]
enum WorkerInterface {
    Ping(Envelope<Ping>),
}

#[derive(Debug, Clone)]
struct Worker;

impl Handler for Worker {
    type Interface = WorkerInterface;
}

impl Handle<Ping> for Worker {
    async fn handle(
        &mut self,
        _ctx: HandlerContext<'_, Self>,
        _msg: Ping,
        _req: (),
    ) -> Result<(), rootcause::Report> {
        Ok(())
    }
}

# #[tokio::main]
# async fn main() {
let node = Node::new(
    Supervisor::blueprint()
        .strategy(SupervisionStrategy::OneForOne)
        .child(Worker.name("worker").unwrap())
        .rand_name(),
);

// In a program this is just `node.run().await`. Here the node runs in the
// background so that the example can inspect it and stop it again.
let root = node.root_supervisor().address().clone();
let running = tokio::spawn(node.run());

// A supervisor counts as running once all of its children have initialized.
root.monitor_init().await.unwrap();
assert_eq!(root.call(GetChildren).await.unwrap().len(), 1);

// The root supervisor exiting is a normal end of the program.
root.signal_shutdown();
assert!(running.await.unwrap().is_ok());
# }
```

A supervisor can also be started like any other actor, without a `Node`, with
`Supervisor::blueprint().start_rand()`, and supervisors can be children of
other supervisors.

To run the program as part of a cluster, use `ClusterNode` instead of `Node`;
see [Running a cluster](distributed/running-a-cluster.md).

## Changing children at runtime

A running supervisor accepts `RegisterChild(spec)` and `DeregisterChild(name)`
messages. For a set of children kept elsewhere, give the blueprint a
`SupervisorSource` with `.source(…)`. `InMemorySupervisorSource` is the
built-in one, and a database-backed source implements the same trait.

## A larger example

`crates/zestors/examples/supervision.rs` builds a tree with:

- nested supervisors with different strategies;
- `Handler` actors that schedule their own ticks;
- closure actors and tasks;
- an `ApiServer`;
- a source that keeps adding tasks.

```rust,ignore
{{#include ../../crates/zestors/examples/supervision.rs:tree}}
```

It runs as follows:

```rust,ignore
{{#include ../../crates/zestors/examples/supervision.rs:run}}
```

```sh
cargo run -p zestors --example supervision
```

## Limits

Supervision is local: a supervisor supervises children in its own process. A
`ChildSpec` holds a local address, and the tree walk uses the local registry.
To watch an actor on another node, use the cross-node `monitor_*` operations;
see [Operating on remote actors](distributed/monitoring.md).
