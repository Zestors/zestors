# `zestors-distr`: structural review and open issues

A handoff note for whoever works on this crate next. It records inconsistencies, dead
ends and restructuring ideas that are not obvious from reading the code, together with
the reasoning behind some choices that look odd but are deliberate.

Everything below was checked against the tree as of this writing; `cargo test --workspace`
is green at 41 suites.

---

## 1. The `Remote*` prefix — resolved

**Done.** The prefix used to split the API along no line at all: the crate began as
"actors on other nodes", then local actors became first-class (`LocalAddress`,
`ClusterAddress`, `ClusterActorOps`) and only the traits got renamed.

The rule now applied: **`Remote*` means crossing the network is the reason the type
exists.** Everything reachable from a purely local send is `Cluster*`.

| was                 | is                   |
| ------------------- | -------------------- |
| `RemoteAccepts`     | `ClusterAccepts`     |
| `RemoteCallOptions` | `ClusterCallOptions` |
| `RemoteReceipt`     | `ClusterReceipt`     |
| `RemoteReply`       | `ClusterReply`       |
| `RemoteOpError`     | `ClusterOpError`     |
| `RemoteCallError`   | `ClusterCallError`   |
| `RemoteCastError`   | `ClusterCastError`   |
| `RemoteReplyError`  | `ClusterReplyError`  |
| `RemoteInfo`        | `ActorInfo`          |

Kept, because remoteness is the property that makes them exist: `RemoteAddress`,
`RemoteError` (it is literally what the peer put on the wire), `RemoteMessage`,
`RemoteSet`, `RemoteMessageKind`, `RemoteRequest`. `CastFailure` was already unprefixed.

`ActorInfo` breaks the `Cluster*` pattern deliberately — `ClusterInfo` would read as
information *about the cluster*, which it is not, and nothing else is called `ActorInfo`.

Two knock-on names were checked and left alone because they are still accurate:
`RemoteMessage::remote_receipt` / `local_receipt` build the receipt for a remote vs a
local send, and `RemoteAddress::cast_remote` is the remote path.

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

Because it is build-time, a `MessageId` claimed by two types **panics** while the node is
being built, in `Handlers::claim`. That is deliberate and not too strict: an id names a
message to the whole cluster, so two types under one id means arriving bytes are decoded
as whichever type won the `insert` — a wrong message delivered silently, or a decode
failure, with nothing in the build to point at the cause. This happened during
development, between two built-in ops, and cost an afternoon. Registering the *same* type
twice is not that, and only logs a warning: it normally means a message registered by hand
as well as by `auto_register`.

## 5. Open issues, roughly by value

### Resolved — cross-node monitors
**Done.** `ClusterActorOps` now has the same `monitor_*` family as `ActorOps`, working for an
actor on another node as well as one here: `monitor_any`, `monitor_exit`, `monitor_running`,
`monitor_accepts_messages`, `monitor_init`.

A closure cannot cross a network, so the primitive is `monitor_any(&[ActorStatusKind])` —
*wait until the status is one of these* — with `ActorStatusKind` being the payload-free
discriminant of `ActorStatus` (`crates/runtime/src/status.rs`). The matched `ActorStatus`
comes back whole, so `Exited` keeps its `ExitStatus`.

On the wire it is `MonitorOp`/`DemonitorOp` in [ops.rs](src/messaging/ops.rs), sent **without a
deadline** — `ClusterReply::Source::Remote` and `Pending::expect` now take
`Option<Duration>`, mirroring the local side, which always did. Losing the node still ends
the monitor with `Disconnected`, which is OTP's `noconnection`.

**Three ways a monitor ends, and all three are needed** (`src/messaging/monitors.rs`):
1. the actor reaches a monitored status;
2. the monitoring side drops the future — a `Demonitor` guard sends `DemonitorOp`, which rides the
   same ordered lane as its `MonitorOp` (both keyed on the target actor), so it cannot
   overtake it;
3. **the monitoring node dies** — nothing announces this, so the holder sweeps per peer via
   `Monitors::drop_node`, next to the existing `Pending::fail_node` in
   [receive.rs](src/messaging/receive.rs). Note the two are mirror images: `fail_node` ends
   calls *this* node made *to* the peer, the sweep ends monitors the *peer* asked *of* this
   node.

Without (3) a monitor would outlive its monitoring node for good, and a restarted node's `monitor_id`
— minted from `next_call`, which restarts at zero — would collide with the stale entry.

Six tests in `tests/remote.rs` cover it, including that a monitor outlives `call_timeout` and
that both cleanup paths actually empty the registry.

**What this unblocks:** a supervisor supervising a child on another node, which was the
reason the gap mattered.

### Medium — a remote cast can be dropped and the sender never told
[receive.rs:340](src/messaging/receive.rs#L340). When the per-actor queue bound is hit a
cast is refused with `Overloaded`; a cast has no reply channel, so the only trace is a
`tracing::warn!` **on the receiving node**. A *local* cast instead waits out the actor's
backpressure.

This is the one place where the documented "at most once" quietly becomes "sometimes zero,
and you will not know". OTP blocks the sender instead (`busy_dist_port`). Minimum: say so
in the `ClusterAccepts` docs. Better: apply backpressure to the peer's lane rather than
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

Still open, related: `ActorInfo` is public and `ChannelState` deliberately is not — a caller
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
- **Run the suite with `cargo nextest run --workspace`** (3-4s), not `cargo test --workspace`
  (27s). nextest does not run doctests, so `cargo test --workspace --doc` is the other half.
  See `CLAUDE.md` at the repo root. The old note here said this suite takes ~105s — that was
  compilation, not the tests, which are seconds.
- Check **both** `--features auto-register` and without: `auto_register.rs` is gated but the
  macro it feeds is always compiled.
- `cargo run -p zestors --example remote` is a good end-to-end smoke test; it should print
  two greetings and "5 letters".
