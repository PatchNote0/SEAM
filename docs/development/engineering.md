# Engineering conventions

How code in this repository is written: architecture boundaries, Rust conventions, errors, the data model, dependencies and tests.

## Architecture

- `seam-core` has no I/O. If a function reads a file or a database, it belongs elsewhere.
- Adapters see one file at a time. Anything needing two files happens in `seam-analysis`.
- Front ends (CLI, desktop) contain presentation only. The same engine call backs both.
- No crate without code that needs it. Merge before splitting.

## Rust

- Encode invariants in types: `HttpMethod` is an enum, a route is `Vec<Segment>`, a Seam id is a newtype that can only be built from a key.
- Borrow by default. Clone only at ownership boundaries such as storing a fact.
- No `unwrap` or `expect` on input-derived values. Allowed only for documented invariants, with the invariant in the message.
- No `unsafe`. If it ever becomes necessary, document the invariant beside it and add a test that would catch its violation.
- Library errors use `thiserror` enums that say what failed and on which path. The CLI may use `anyhow` at its top level only.
- `cargo clippy -- -D warnings` and `cargo fmt` must pass. Do not silence a lint without a comment explaining why the code is right.

## Data model

- Every public analysis result serialises with serde and round-trips.
- Sort before you report or store: by path, then span start, then key.
- Ids are hashes of keys, never of line numbers or timestamps.
- New fields in stored JSON must have serde defaults so old snapshots still load.

## Dependencies

Prefer widely used, maintained crates. Write the reason for a new dependency in the commit message. Keep the engine free of HTTP clients and async runtimes until a feature needs them (ADR 0005).

## Tests

- Name tests after the behaviour: `wrapper_call_resolves_to_route_through_prefix`.
- Test outcomes, not the implementation's own assumptions. Each matcher needs a negative test proving it does not fire on look-alike text.
- Parser tests include malformed input that must not panic.
- Fixture-level tests live in `crates/seam-analysis/tests` and read `fixtures/polyglot`.
