# Contributing

## Commands

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo run -p seam-cli -- scan fixtures/polyglot
```

## Layout

- `crates/seam-core`: domain model and pure rules (routes, shapes, compatibility, classification).
- `crates/seam-adapters`: per-file extraction (TypeScript, Python, OpenAPI, Compose).
- `crates/seam-analysis`: discovery, linking, assembly of Seams.
- `crates/seam-storage`: SQLite snapshots, lifecycle, ledger queries.
- `crates/seam-cli`: the `seam` binary.
- `fixtures/polyglot`: the demonstration system with documented defects.
- `docs/`: architecture, data model, decisions, plan and status.

## Ground rules

- Read the relevant crate, its tests and `docs/` before changing it. Check `git status` before large edits.
- Never fabricate evidence, line numbers, test results or benchmark numbers. If something was not run, say so.
- A relationship found by text similarity is not a Seam. Matching rules live in `seam-core` and `seam-analysis` and are documented in `docs/DATA_MODEL.md`.
- Never label an inferred relationship VERIFIED. Assumptions lower the class; deterministic resolution does not.
- The fixture's defects are documented in `fixtures/polyglot/FIXTURE.md`. Production code must not mention them; tests assert them.
- Red, green, refactor: write the failing test, watch it fail for the right reason, make it pass, run the whole suite, read the diff.
- No new dependency without a reason recorded in the commit message or an ADR.

## Definition of done

A feature is done only when it is implemented, has tests that were run and
pass, handles its errors, is documented accurately in `docs/`, has its
limitations written down, and any interface for it is wired to real
behaviour. Update `docs/PROJECT_STATUS.md` in the same change.

## Guides

- [Engineering conventions](docs/development/engineering.md)
- [Adding a language adapter](docs/development/language-adapters.md)
- [Reviewing a Seam](docs/development/reviewing-seams.md)
- [Evidence quality](docs/development/evidence-quality.md)
- [Release checklist](docs/development/release-checklist.md)
