# `zestors-distr`: structural review and open issues

A handoff note for whoever works on this crate next. It records inconsistencies, dead
ends and restructuring ideas that are not obvious from reading the code, together with
the reasoning behind some choices that look odd but are deliberate.

Everything below was checked against the tree as of this writing; `cargo test --workspace`
is green at 41 suites.

---

## 1. The `Remote*` prefix no longer means anything

This is the biggest legibility problem in the public API, and it is recent.

The crate used to be about actors on *other* nodes, so everything was `Remote*`. Since
then, actors on *this* node became first-class: `LocalAddress`, `ClusterAddress`,
`ClusterActorRef`, `ClusterActorOps`. The traits that span both were renamed to
`Cluster*`. The rest were not, so the prefix now splits the API along no line at all.

**Genuinely remote-only** — the prefix is correct:
- `RemoteAddress` — an actor on another node.
- `RemoteError` — what the *remote node* reported; it travels on the wire
  (`frame.rs` encodes it).
- `RemoteMessage`, `RemoteSet` — "can cross a network", a property of the type.

**Applies equally to a local actor** — the prefix is now misleading:
- `RemoteAccepts` — blanket-implemented for every `ClusterActorRef`, which includes
  `LocalAddress` ([send.rs:292](src/messaging/send.rs#L292)).
- `RemoteInfo` — returned by `ClusterActorOps::info()` for local actors too.
- `RemoteReply`, `RemoteReceipt` — `RemoteReply` has a `Source::Local` variant.
- `RemoteOpError`, `RemoteCallError`, `RemoteCastError`, `CastFailure`,
  `RemoteCallOptions` — all reachable from a purely local send.
- `RemoteRequest` — a reply channel inside a message; works locally.

**Suggested rule**, if this gets cleaned up: the prefix answers *"does this concept still
exist when the actor is on this node?"* If yes, it is `Cluster*` or unprefixed; if no, it
is `Remote*`. That would rename roughly nine public types. It is a large but purely
mechanical break, and it is much cheaper now than after the crate has users.

Do not do this piecemeal — half-renamed is worse than either end state.

## 2. `Cluster` is one type implemented in two files

`Cluster` is a handle over `ClusterInner { local_name, members: MembershipView,
messaging: CommunicationView }`:
- [`cluster/state.rs`](src/cluster/state.rs) — the type, plus two `impl Cluster` blocks
  for membership (lines 47 and 228).
- [`messaging/node.rs`](src/messaging/node.rs) — a third `impl Cluster` for messaging
  (line 63), plus `CommunicationView` and `Serving`.

The split is **deliberate and worth keeping**: it is what stops `cluster/` from depending
on `messaging/`. But `messaging/node.rs` is a poor name for "the messaging half of
`Cluster` plus its runtime state" — it reads as if it were about `ClusterNode`, which
lives in `cluster/node/mod.rs`. Two different `node`s.

**Suggestion:** rename `messaging/node.rs` → `messaging/cluster.rs` and say in its module
doc that `Cluster`'s membership half lives in `cluster/state.rs`. One-line change, removes
a real stumbling block.

## 3. Dead code

- **`ClusterAddressMut` and `ClusterActorRef::as_ref_mut`**
  ([address.rs:227](src/messaging/address.rs#L227), [:245](src/messaging/address.rs#L245))
  are defined and implemented three times but **never called**. Either they are scaffolding
  for planned work — in which case say so — or they should go.
- **`Cluster::membership_only`** ([state.rs:72](src/cluster/state.rs#L72)) warns as dead
  code on any build without `--features sim`. It is used only by `sim` and by a
  `#[cfg(test)]` test in `cluster/membership/driver.rs`. Gate it:
  `#[cfg(any(feature = "sim", test))]`.
- **`Cargo.toml` feature docs are stale**: the `auto-register` comment still refers to
  `Cluster::auto_register`, which moved to `ClusterConfig::auto_register`. And `_ra =
  ["auto-register"]` is undocumented — it looks like a rust-analyzer workaround; say so or
  drop it.

## 4. Things that look like bugs but are not — leave them alone, document them

Two behaviours read as inconsistent and have already been investigated. Do not "fix" them
without reading this first.

### `RemoteSet` requires *every* message in an interface to be remote-capable
`Cluster::address::<I>()` needs a `MessageId` per member of `I::Set`, and only a
`RemoteMessage` has one. An interface mixing a remote message with a local-only one
therefore does not compile at the `address::<I>()` call; you reach its remote half with
`address_dyn::<(TheRemoteOnes,)>`.

This looks fixable by "just skipping" the non-remote members. **It is not, on stable
Rust.** Doing so needs autoref specialization, which resolves at type-check time of the
*generic* body — inside `impl<A, B> RemoteSet for (A, B)`, `A: StableId` is not known to
hold, so the probe always takes the negative branch and you silently get an empty list.
This was verified with a standalone repro. It works only at a concrete call site, which is
why the `Probe`/`IfRemote`/`IfNot` trio in [auto_register.rs](src/messaging/auto_register.rs)
works from a derive macro and cannot be reused here. Nightly `specialization` does work,
but the project does not want nightly-only features.

The three workable shapes, if this is revisited: keep it strict (status quo); a
`#[derive(RemoteInterface)]` that emits the id list from concrete types; or a runtime
`TypeId → MessageId` table populated unconditionally by the `StableId` derive.

### Registration is build-time only, on purpose
`ClusterConfig::register::<M>()` / `.auto_register()`, not on a running `Cluster`. This is
not an oversight — it closes a window where a node could serve before its handlers exist,
and it is what lets `Handlers` be a plain `IndexMap` with no locking
([dispatch.rs](src/messaging/dispatch.rs)). Do not add a `Cluster::register` back.

## 5. Open issues, roughly by value

### High — no cross-node monitors
This is the framework's largest gap against OTP, which it is modelled on. OTP builds
distribution on `erlang:monitor(process, {Name, Node})` → `{'DOWN', Ref, process, Pid,
Reason}`, with `noconnection` when the node goes; `gen_server:call`, supervisors and
`global` all sit on that one primitive. There is no equivalent here. Consequences:

- A supervisor cannot supervise a child on another node — supervision stops at the process
  boundary.
- `RemoteAddress` cannot offer `watch_exit`, which `ActorOps` has locally, so
  `ClusterAddress` can never be fully location-transparent.
- Callers must pick a timeout instead of learning the truth; a node that is up but wedged
  holds every call for the full `call_timeout`.

**Shape that fits the existing code:** a `MonitorOp`/`DemonitorOp` beside the built-ins in
[ops.rs](src/messaging/ops.rs); a `Monitors` table beside `Pending` in
[pending.rs](src/messaging/pending.rs), keyed in the same id space and failed on the same
`fail_node` path; and `ClusterActorOps::monitor()` resolving locally via the existing
`ActorOps::watch_exit`.

### Medium — a remote cast can be dropped and the sender never told
[receive.rs:340](src/messaging/receive.rs#L340). When the per-actor queue bound is hit a
cast is refused with `Overloaded`; a cast has no reply channel, so the only trace is a
`tracing::warn!` **on the receiving node**. A *local* cast instead waits out the actor's
backpressure.

This is the one place where the documented "at most once" quietly becomes "sometimes zero,
and you will not know". OTP blocks the sender instead (`busy_dist_port`). Minimum: say so
in the `RemoteAccepts` docs. Better: apply backpressure to the peer's lane rather than
dropping.

### Medium — local and remote calls have different timeout rules
A call through a remote address gets the node's `call_timeout` (30s default); a call
through a local one has **no** default timeout, and `ClusterAddress::with_timeout` silently
ignores the local case. OTP's `gen_server:call` is 5s either way.

`LocalAddress` now exists and is the obvious place for an `Option<Duration>`, seeded from
the cluster's `call_timeout` at construction. Note `Route::Local`/`ClusterAddressRef::Local`
would have to carry it through to the local `cast`/`try_cast` in
[send.rs](src/messaging/send.rs).

### Medium — `auto_register` skips non-remote types in silence
[auto_register.rs:52](src/messaging/auto_register.rs#L52): `IfNot::register` is an empty
default. A message that derives `StableId` but is missing `Encode`/`Decode` registers
nowhere and fails much later with `RemoteError::UnknownMessage`, far from the cause.
Deriving `StableId` says "this type has a wire identity", so being unable to cross the wire
is almost certainly a mistake worth reporting.

**Fix:** have `IfRemote`/`IfNot::register` return `Option<&'static str>` — `None` when
registered, `Some(type_name::<M>())` when skipped — and have `ClusterConfig::auto_register`
emit one `tracing::warn!` naming them. Adding a `ClusterConfig::auto_register_skipped() ->
Vec<&'static str>` makes it testable and lets a user assert on it at startup.

### Low — two disconnect paths, no stated owner
[receive.rs:101](src/messaging/receive.rs#L101) fails a node's pending calls on
`PeerEvent::Disconnected`. [driver.rs:129](src/cluster/membership/driver.rs#L129) treats the
same event as a non-event, with a comment that "Nothing here rides on a single connection".

**Both are correct.** Failing calls on a dropped connection matches OTP: a reply already in
flight is gone and cannot be distinguished from one not yet sent. Membership genuinely does
not ride on one connection — it has its own failure detector. The defect is only that
neither says who owns "calls to this node are hopeless", and the driver's comment reads as
if *nothing* rides on a connection, which is false for messaging. Comments plus a test that
pins the behaviour; no behaviour change.

### Low — two different shard functions in one protocol
A request's lane is `hash(name) % shards` chosen by the sender; a reply's lane is
`call_id % shards` chosen by the replier ([receive.rs:160](src/messaging/receive.rs#L160)),
and `shards` is each node's own `lanes` config, so the two sides can disagree on the count.

This is correct — a lane only orders what passes through it, and replies are matched by
`call_id`, not by lane — but nothing says so, and the pairing invites the assumption that
both sides must agree. One comment.

## 6. Done: narrow built-in ops, instead of always asking for the full info

`Cluster::address()` used to resolve a remote actor with `is_superset_of(ids)`, which went
`ClusterActorOps::members()` → `info()` → an `InfoOp` round trip that returned **the whole
accepts list**, only for the caller to test `k` ids against it. Every one-field read
(`status`, `msg_len`, `reached_backpressure`, …) paid the same price: the actor's name,
its bounded spawn and exit history, and the accepts list, for one number.

Two built-ins beside the old three in [ops.rs](src/messaging/ops.rs) now answer directly:

- `AcceptsOp(Vec<MessageId>) -> bool` — "do you accept these `k`?", answered on the
  target node against `Handlers`. Behind `accepts`, `accepts_id`, `is_superset_of`, and
  so behind every `Cluster::address()` / `address_dyn()` call.
- `StateOp -> ChannelState` — the live counters only (`status`, `msg_len`, `signal_len`,
  `reached_backpressure`); no name, no history, no accepts list. Behind `status`,
  `is_exiting`, `is_dead`, `msg_len`, `signal_len` and `reached_backpressure`.

`InfoOp` stays, and is now what it should always have been: the operation you use when you
want several things at one instant, or the two things only it carries — `snapshot()` /
`last_spawned_at()` and `members()`.

`Handlers` lost its second way of answering "does this actor accept `id`?". It had two:
`Handler::accepts` (a dyn call into `address.accepts::<M>()`) for the single-id question,
and the `by_type` map for the whole list. They could not disagree, but only by accident.
Both are now `Handlers::registered_ids`, with `accepted_ids` and `accepts_ids` over it, and
`Handler::accepts` is gone. A `k`-id question maps the actor's types once and binary-searches
`k` times, rather than rescanning the actor's members per id — which is also what the local
`is_superset_of` did.

Pinned by `the_narrow_operations_agree_with_a_full_info` in `tests/remote.rs`: every narrow
op gives exactly what the matching `info()` field would, for an accepted message, a
registered-but-unaccepted one, and an unregistered one.

Still open, related: `RemoteInfo` is public and `ChannelState` deliberately is not — a caller
reads it one field at a time. If it is ever made public, it is a `Cluster*`/unprefixed name
under §1's rule, not a `Remote*` one.

## 7. Testing notes for whoever changes this crate

- **Several clusters run in one OS process.** `sim` is a shipped, feature-gated module
  whose whole point is that (`SimNetwork`, virtual time via `start_paused`, seeded
  determinism, `net.partition()`). `Pair` in `tests/remote.rs` runs node-a and node-b side
  by side. **Do not make `Cluster` a process-global** the way `Registry::local()` is — it
  would delete that capability and the whole distr suite with it. `Registry` gets away with
  it because a registry is genuinely process-wide and needs no configuration; a `Cluster`
  needs `local_name`, `call_timeout` and `shards` before it can exist.
- `cargo test --workspace` takes ~105s; `cluster.rs` is slow. Do not kill it early.
- Check **both** `--features auto-register` and without: `auto_register.rs` is gated but the
  macro it feeds is always compiled.
- `cargo run -p zestors --example remote` is a good end-to-end smoke test; it should print
  two greetings and "5 letters".
