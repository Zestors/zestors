# Membership and events

`Cluster` shows which other nodes are up:

- `members()` lists them, and `member(name)` looks one up;
- `is_reachable(name)` tells whether this node can currently connect to it;
- `wait_for_members(n)` waits until exactly `n` *other* nodes are up;
- `wait_until(condition)` waits for any condition on the member list.

## Events

`subscribe()` returns a stream of `ClusterEvent`s:

| Event | Meaning |
| --- | --- |
| `Up(member)` | a node joined, or came back after being declared down |
| `Left(member)` | a node announced that it was shutting down |
| `Failed(member)` | the failure detector declared a node down: it crashed, or can't be reached by anyone |
| `Unreachable(member)` | *this* node can't connect to a node that is still up |
| `Reachable(member)` | … and now it can again |

A node restarted before anyone noticed shows up as `Failed` for the old
incarnation, followed by `Up` for the new one.

The stream is a broadcast channel: a receiver that falls far behind misses
events. To know the members right now *and* every change after that, use
`subscribe_with_snapshot()`. Each change is then in either the snapshot or the
stream, never both and never neither. Calling `members()` and `subscribe()`
separately can miss a change in between.

```rust,ignore
{{#include ../../../crates/zestors/examples/cluster.rs:config}}
```

Run it in a few terminals to watch nodes join, and press Ctrl+C or `kill -9` one
of them to see the others notice it leave or fail:

```sh
cargo run -p zestors --features distr --example cluster -- node-a 127.0.0.1:7001
cargo run -p zestors --features distr --example cluster -- node-b 127.0.0.1:7002 node-a=127.0.0.1:7001
cargo run -p zestors --features distr --example cluster -- node-c 127.0.0.1:7003 node-a=127.0.0.1:7001
```

## Failure detection

Membership uses SWIM: nodes probe each other at random, suspect a node that
stops answering, and declare it down once the suspicion isn't refuted in time.
How quickly a crash is noticed depends on the membership settings.
`ClusterConfig::expected_size` tunes the defaults for a cluster of that size,
and `foca_config` replaces them entirely.

A node declared down that can't rejoin becomes `NodeStatus::Defunct`. It keeps
running, but no longer takes part in the cluster; restart it.

A node's *incarnation* grows with every start, so peers can tell a restarted
node from the old one. It is based on the clock. If the clock can be set back
between restarts, use `ClusterConfig::generation_store(path)` to persist it.
