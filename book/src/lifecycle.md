# Lifecycle and signals

## Statuses

An actor is always in one `ActorStatus`:

```text
Initializing ──► Running ◄──► Suspended
      │             │             │
      └─────────────┴──► Exiting ─┴──► Exited(reason)
```

- **Initializing**: spawned, but it hasn't asked its inbox for anything yet.
  Messages are accepted and queued.
- **Running**: it has called a receiving method (`recv`, `recv_event`, …) at
  least once.
- **Suspended**: it received `Signal::Suspend`, and stops receiving messages
  until `Signal::Resume`.
- **Exiting**: it received `Signal::Shutdown`. New messages are refused, and the
  ones already queued are still delivered.
- **Exited**: the task has finished. The `ExitStatus` says how: normally, with
  an error, by panicking, or by being aborted.

`ActorStatusKind` is the same list without the exit reason. It is what you
name when waiting for a status.

## Waiting for a status

`ActorOps` has a family of `monitor_*` methods that wait for the actor to reach
a status:

| Method | Returns when the actor | Result |
| --- | --- | --- |
| `monitor_init()` | first becomes `Running` | `Err(ExitStatus)` if it exited first |
| `monitor_running()` | is `Running` | — |
| `monitor_accepts_messages()` | is `Initializing`, `Running` or `Suspended` | — |
| `monitor_exit()` | has `Exited` | the exit as a `Result` |
| `monitor_any(&[kinds])` | is in any of `kinds` | the `ActorStatus` it reached |
| `monitor(f)` | makes the closure `f` return `Some` | what `f` returned |

Each one first checks the current status, so it returns at once if the actor is
already there. The same family exists for actors on other nodes; see
[Operating on remote actors](distributed/monitoring.md).

```rust
use zestors::prelude::*;
use zestors::runtime::{ActorStatus, ActorStatusKind, spawn_rand};

# #[tokio::main]
# async fn main() {
let child = spawn_rand(|mut inbox: Inbox<()>| async move {
    while inbox.recv().await.is_some() {}
    Ok(())
});

// Code right after a spawn must not assume the actor is running yet.
child.monitor_init().await.unwrap();

child.signal_suspend();
let status = child
    .monitor_any(&[ActorStatusKind::Suspended, ActorStatusKind::Exited])
    .await;
assert_eq!(status, ActorStatus::Suspended);

child.signal_shutdown();
child.monitor_exit().await.unwrap();
# }
```

## Signals

Signals control an actor rather than asking it to do work:

- `signal_shutdown()` → `Signal::Shutdown`
- `signal_suspend()` → `Signal::Suspend`
- `signal_resume()` → `Signal::Resume`

`ping()` also exists: the runtime answers it by itself, and the actor never
sees it.

Two behaviours surprise people:

**A signal takes effect later.** `signal_shutdown()` puts the signal in the
actor's queue and returns. `status()` on the next line can still show the old
status. To wait for the effect, use `monitor_exit()` or another `monitor_*`.

**A signal sent too early is dropped.** A name can exist before its actor has
been spawned: a `ChildSpec` reserves it, and a supervisor spawns the actor
later. Until then the status is `Exited`, and a signal is refused (the method
returns `false`). When you might race with a start, call
`monitor_accepts_messages()` first.

### Priority

Signals are delivered ahead of queued messages, so a shutdown doesn't wait
behind a backlog. Once a shutdown has been received:

- `recv()` and `recv_event()` keep delivering the messages that were already
  queued, then return `None`;
- the actor exits as soon as the queue is empty, without looking at any signal
  still behind the shutdown.

An actor that has to keep handling signals after a shutdown should loop over
`recv_event_always()`, and decide for itself when to stop.

## Shutting down with a deadline

`Child::shutdown_abort(timeout)` sends a shutdown, waits up to `timeout` for the
actor to exit, and aborts it if it doesn't. Supervisors use the same rule when
they stop a child, with the child's `abort_timeout`.
