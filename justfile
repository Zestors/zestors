[private]
default:
    @just -l -u --list-submodules

# Run every test: nextest for unit and integration tests, then the doctests
# (including the book's code blocks, via the zestors-book crate).
test:
    cargo nextest run --workspace
    cargo test --workspace --doc

# Serve the book at http://localhost:3000, rebuilding on changes.
book:
    mdbook serve book --open

# Build the API docs the way docs.rs does, failing on broken links.
doc:
    RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links --cfg docsrs" cargo doc --workspace --all-features --no-deps

# A supervision tree with the HTTP API on :8080, for the inspector.
supervise-example:
    @cargo run --example supervision

# Two cluster nodes in one process; prints two greetings and "5 letters".
remote-example:
    @cargo run --example remote

mod inspector "crates/inspector"
