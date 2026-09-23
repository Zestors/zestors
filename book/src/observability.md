# Observability

## Inspecting from code

Every reference can report on its actor without disturbing it:

- `status()`, `msg_len()`, `signal_len()` and `reached_backpressure()` read
  single values;
- `snapshot()` returns a `ChannelSnapshot` with the status, queue lengths, and
  the recent spawn and exit history.

A running tree can be walked with `SupervisionTree`. It starts from a
supervisor's `ChildDescription` and asks each supervisor for its children with
`GetChildren`; see [Dynamic addresses](dynamic-addresses.md#how-the-supervision-tree-uses-this).
Any actor can take part in health reporting by accepting `GetHealth` and
replying with a `Health`: healthy, degraded or unhealthy, with an optional
message and details.

## The HTTP API server

`ApiServer` is an actor that serves the tree over HTTP. Add it as a child next
to the rest of the tree, and give it the name of the root supervisor:

```rust,ignore
Supervisor::blueprint()
    .child(ApiServer::blueprint("127.0.0.1:8080".parse()?, "root-supervisor").name("api-server")?)
    .child(app_supervisor)
    .name("root-supervisor")?
```

| Route | Returns |
| --- | --- |
| `GET /processes` | every actor in the tree, with its status and child configuration |
| `GET /snapshots` | a `ChannelSnapshot` per name, or `null` if it is gone |
| `GET /health` | a `Health` per name, or `null` if it is gone or didn't answer in time |

The last two take a JSON array of names as the request body.

The routes are not stable yet and may change in any minor release.

## The inspector

`zestors-inspector` is a desktop GUI, still a proof of concept, that draws the
tree served by the API server and polls it for changes. It connects to
`http://localhost:8080`.

```sh
cargo run -p zestors --example supervision   # in one terminal
just inspector run                           # in another; or: cargo run --release -p zestors-inspector
```

![The inspector showing a running supervision tree](https://raw.githubusercontent.com/Zestors/zestors/main/images/inspector.png)

The API server and the inspector see one process. In a cluster, each node runs
its own.
