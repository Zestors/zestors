# Handler actors

Writing the receive loop by hand gives full control. Most actors don't need
it. `Handler` provides the loop, and asks for one `Handle<M>` implementation
per message.

```rust
use zestors::interface::{Envelope, Interface, Message, Request};
use zestors::prelude::*;

#[derive(Message, Debug)]
struct Increment;

#[derive(Message, Debug)]
#[msg(reply = u32)]
struct GetCount;

#[derive(Interface, HandlerInterface, Debug)]
enum CounterInterface {
    Increment(Envelope<Increment>),
    GetCount(Envelope<GetCount>),
}

#[derive(Debug, Clone)]
struct Counter {
    count: u32,
}

impl Handler for Counter {
    type Interface = CounterInterface;
}

impl Handle<Increment> for Counter {
    async fn handle(
        &mut self,
        _ctx: HandlerContext<'_, Self>,
        _msg: Increment,
        _req: (),
    ) -> Result<(), rootcause::Report> {
        self.count += 1;
        Ok(())
    }
}

impl Handle<GetCount> for Counter {
    async fn handle(
        &mut self,
        _ctx: HandlerContext<'_, Self>,
        _msg: GetCount,
        req: Request<u32>,
    ) -> Result<(), rootcause::Report> {
        let _ = req.reply(self.count);
        Ok(())
    }
}

# #[tokio::main]
# async fn main() {
let child = Counter { count: 0 }.spawn_rand();
for _ in 0..5 {
    child.cast(Increment).await.unwrap();
}
assert_eq!(child.call(GetCount).await.unwrap(), 5);
child.signal_shutdown();
# }
```

- `#[derive(HandlerInterface)]` on the interface dispatches each variant to the
  matching `Handle<M>`. It goes next to `#[derive(Interface)]`.
- The third argument of `handle` is the message's reply handle: `()` for a
  fire-and-forget message, `Request<T>` for one with a reply.
- A handler returns `Result<(), rootcause::Report>`. An error stops the actor.
- Every `Handler` is an `Actor`. It is spawned with `ActorExt::spawn` or
  `spawn_rand`, and awaited or messaged like any other actor.
- A `Handler` that is `Clone` is also a `Blueprint`, so it can be
  [supervised](supervision.md) as it is.

## Lifecycle hooks

All hooks are optional:

| Hook | Called |
| --- | --- |
| `init` | once, before the first message is handled |
| `on_shutdown` | when `Signal::Shutdown` arrives; the actor is already `Exiting` |
| `on_suspend` / `on_resume` | when those signals arrive |
| `exit` | when the loop ends, with a `HandlerExit` saying why |

`exit` receives one of these:

- `Normal`: after a shutdown, or once the inbox has closed;
- `InitError`: `init` returned an error;
- `InitCancelled`: a shutdown arrived while `init` was still running, so `init`
  was cancelled;
- `HandlerError`: a handler or hook returned an error.

The default `exit` turns any of these but `Normal` into an error. `exit` is
not called when the actor panics or is aborted.

`init` runs alongside the inbox's signal receiver, so the actor may already
report `Running` while `init` is still busy. Don't use `monitor_init()` as a
signal that `init` has finished.

## Reacting to more than messages

`Handler::next_event` lets the actor react to other futures between messages: a
timer, a stream, a background job. Return `Some(Ok(event))` to have `event`
handled like a message. `event` can be any message the handler has a `Handle`
for.

```rust
use std::time::Duration;
use zestors::interface::{Envelope, Interface, Message, Request};
use zestors::prelude::*;

#[derive(Message, Debug)]
struct Tick;

#[derive(Message, Debug)]
#[msg(reply = u32)]
struct GetTicks;

#[derive(Interface, HandlerInterface, Debug)]
enum TickerInterface {
    GetTicks(Envelope<GetTicks>),
}

#[derive(Debug)]
struct Ticker {
    ticks: u32,
    interval: tokio::time::Interval,
}

impl Handler for Ticker {
    type Interface = TickerInterface;

    async fn next_event(&mut self) -> Option<Result<impl HandledBy<Self>, rootcause::Report>> {
        self.interval.tick().await;
        Some(Ok(Tick))
    }
}

impl Handle<Tick> for Ticker {
    async fn handle(&mut self, _: HandlerContext<'_, Self>, _: Tick, _: ()) -> Result<(), rootcause::Report> {
        self.ticks += 1;
        Ok(())
    }
}

impl Handle<GetTicks> for Ticker {
    async fn handle(
        &mut self,
        _: HandlerContext<'_, Self>,
        _: GetTicks,
        req: Request<u32>,
    ) -> Result<(), rootcause::Report> {
        let _ = req.reply(self.ticks);
        Ok(())
    }
}

# #[tokio::main(flavor = "current_thread", start_paused = true)]
# async fn main() {
let ticker = Ticker { ticks: 0, interval: tokio::time::interval(Duration::from_secs(1)) };
let child = ticker.spawn_rand();

tokio::time::sleep(Duration::from_millis(3500)).await;
assert!(child.call(GetTicks).await.unwrap() >= 3);
child.signal_shutdown();
# }
```

`Tick` is not part of the interface, so nobody else can send it. The future
returned by `next_event` is dropped whenever a message or signal arrives first,
so it must be cancellation-safe.

For several concurrent futures, keep a `BasicScheduler` in the handler and
return `self.scheduler.next().await` from `next_event`. It runs scheduled
futures and hands their results to the handler: messages with `schedule_msg`,
closures over the handler's state with `schedule_callback`.

## Writing the loop yourself

`Handler` covers most actors. Implement `Actor` directly — its `run` takes the
`Inbox` — when the actor needs a receive order `Handler` doesn't offer, for
example handling signals only between batches of messages. Spawning a closure,
as in [Spawning and messaging actors](actors.md), is the same thing without a
named type.
