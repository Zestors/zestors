# zestors

## Running tests: use `cargo nextest`, not `cargo test`

```sh
cargo nextest run --workspace        # every test except doctests
cargo test --workspace --doc         # the doctests, which nextest cannot run
```

`cargo nextest` is installed. Use it. Warm timings for the whole workspace, measured
2026-09-20:

| command | wall time | what it runs |
| --- | --- | --- |
| `cargo nextest run --workspace` | **3-4s** | 199 unit + integration tests |
| `cargo test --workspace` | 27-28s | the same 199, plus 37 doctests |
| `cargo test --workspace --doc` | 16s | the 37 doctests alone |

`cargo test` runs each test binary one after another, so ~12s of its time is the same 199
tests nextest finishes in 3. It is worth the switch on every run, not just long ones.

**nextest does not run doctests.** This crate has 37 of them and several are real API
examples, so a change to public API or its docs is not verified until
`cargo test --workspace --doc` has also passed. Run both before claiming a green suite.

Other things nextest gives that matter here:

- A per-test time on every line, so a slow test names itself. `--no-fail-fast` keeps going
  after a failure instead of hiding the rest.
- Each test runs in its own process. The actor tests lean on process-global state
  (`Registry::local()` is process-wide, and actor names must be unique within a process),
  so this isolation is a real safety margin, not just speed.
- Reruns: `cargo nextest run -E 'test(the_name)'` for one test,
  `-p zestors-distr` to scope to a crate.

## Test suite notes

- **The distr suite is the slow part**, and its slowest tests are genuinely doing work:
  `the_number_of_lanes_can_be_chosen` (~2s, five lane configurations × six actors × 100
  casts), `nodes_discover_each_other_and_notice_departures` (~1.7s, foca gossip),
  `a_node_cannot_get_in_under_a_name_the_backend_does_not_know_it_by` (~1.4s). Most distr
  tests use `#[tokio::test(start_paused = true)]`, so their *virtual* time is free; what
  costs is the real work — gossip loops, and real QUIC handshakes in `remote_quic`.
- Check `zestors-distr` **both** with and without `--features auto-register`:
  `auto_register.rs` is gated but the macro feeding it is always compiled.
  `--no-default-features --features sim` is the other side to test.
- `cargo run -p zestors --example remote` is a good end-to-end smoke test; it should print
  two greetings and "5 letters".
- `crates/distr/REVIEW.md` is a handoff note on that crate's open issues and on behaviours
  that look like bugs but are deliberate. Read it before "fixing" anything in `distr`.
