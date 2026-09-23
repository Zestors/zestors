# zestors

## Running tests: use `cargo nextest`, not `cargo test`

```sh
cargo nextest run --workspace        # every test except doctests
cargo test --workspace --doc         # the doctests, which nextest cannot run
```

`cargo nextest` is installed. Use it. `just test` runs both commands. Rough warm timings
for the whole workspace (counts grow; these are from 2026-09):

| command | wall time | what it runs |
| --- | --- | --- |
| `cargo nextest run --workspace` | **3-5s** | ~210 unit + integration tests |
| `cargo test --workspace` | ~30s | the same tests, one binary after another, plus the doctests |
| `cargo test --workspace --doc` | ~20s | the doctests alone |

`cargo test` runs each test binary one after another, so much of its time is the same
tests nextest finishes in seconds. It is worth the switch on every run, not just long ones.

**nextest does not run doctests.** The workspace has many of them, and several are real
API examples, so a change to public API or its docs is not verified until
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
- `zestors-distr`'s tests always build with `sim` and `auto-register` (its dev-dependency
  on itself enables both), so the feature-less build is only checked by
  `cargo check -p zestors-distr`. Run that too: `auto_register.rs` is gated, but the macro
  feeding it is always compiled.
- `cargo run -p zestors --example remote` is a good end-to-end smoke test; it should print
  two greetings and "5 letters".
- `cargo test -p zestors-book --doc` runs the code in the book and the README; see below.
- `crates/distr/REVIEW.md` is a handoff note on that crate's open issues and on behaviours
  that look like bugs but are deliberate. Read it before "fixing" anything in `distr`.

## Documentation

- **The book** (`book/`, mdBook, published to GitHub Pages by `.github/workflows/book.yml`)
  is the guide: concepts, walkthroughs, larger examples. `mdbook build book` or `just book`.
- **Rustdoc** is the reference: keep crate docs short, one small example each, and link to
  the book for walkthroughs rather than repeating them.
- Every ```` ```rust ```` block in `book/src` and in `README.md` is compiled and run as a
  doctest by the `zestors-book` crate (`book/lib.rs`). A new chapter must be added to the
  list there. Blocks that pull in example code with `{{#include}}` are marked
  `rust,ignore`; that code is compiled as the examples in `crates/zestors/examples`, and
  referenced through `// ANCHOR:` markers — keep those intact when editing an example.
- Distributed examples in the book run on the `sim` network with
  `#[tokio::main(flavor = "current_thread", start_paused = true)]`, so they take no real
  time.
- `just doc` builds the API docs as docs.rs does and fails on broken intra-doc links.
