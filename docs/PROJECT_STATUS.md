# Project status

Updated: 2026-11-01. This page says what exists and what does not. If it
disagrees with the code, the page is wrong and must be fixed.

## Built and tested

| Area | What exists | Evidence |
|---|---|---|
| Phase 0 documents | Product differentiation, architecture, data model, security, implementation plan, ADRs 0001 to 0006, development guides. | `docs/` |
| Polyglot fixture | Ledgerline: TypeScript web client, FastAPI API, Rust Axum ledger, Postgres migrations, OpenAPI, Compose. Defects 1, 2 and 5 are planted and documented; 3 and 4 are planted for later phases. | API pytest: 3 passed. Web: `tsc --noEmit` clean, Vitest 2 passed. Commands in `fixtures/polyglot/FIXTURE.md`. |
| `seam-core` | Route templates and matching, shapes, directional field comparison with findings, evidence and content hashes, the Seam model, stable ids, classification and lifecycle rules. A `number` written where an `integer` is read is left undecided. | `cargo test -p seam-core`: 32 passed. Clippy with `-D warnings` and rustfmt clean. |
| `seam-adapters` | Per-file extraction with spans: TypeScript and JavaScript (requests through `fetch` and `axios`, wrapper candidates, URL constants and environment fallbacks, interfaces and type aliases as shapes, imports, test cases), Python (FastAPI apps, routers, routes, includes, Pydantic-style classes with aliases, imports, TestClient requests), OpenAPI, Docker Compose and manifests. File kinds by path, one `extract` entry point that never panics on malformed input, and declared capabilities. | `cargo test -p seam-adapters`: 36 passed. Clippy with `-D warnings` and rustfmt clean. |
| `seam-analysis` | Discovery (`.gitignore`, `.seamignore`, `--exclude` patterns, secret files skipped and reported, 2 MiB limit, binary and non-UTF-8 files skipped, no symlinks), parallel extraction with cancellation, components from manifests, Compose services and environment, FastAPI router prefixes and includes across modules, Pydantic models with aliases and inheritance, OpenAPI operations attached to routes, TypeScript wrappers (including wrappers of wrappers), constants and environment fallbacks, consumer and producer tests, Seam assembly with evidence, assumptions, classification, confidence and Persian renderings of every sentence. | `cargo test -p seam-analysis`: 24 passed, including the fixture's documented defects 1, 2 and 5, the nullable receipt field, the alias, the UNKNOWN search call, evidence lines and hashes checked against the files, and identical results across two scans. |
| `seam-storage` | SQLite at `<repo>/.seam/seam.db` (bundled SQLite), numbered migrations in `PRAGMA user_version`, snapshots, files, observations with the full Seam JSON, evidence and findings tables, lifecycle against the previous snapshot, removed Seams kept as history, the ledger query, lookup by id prefix, refusal of a newer schema. The `.seam` folder ignores itself for Git. | `cargo test -p seam-storage`: 4 passed. |

## Not built yet

- `seam-cli`: the `seam` command.
- Fixture defect 6 (scripted Git history).
- Change Rehearsal, Git integration, file watching, the desktop application.

No language is supported yet in the sense of `docs/LANGUAGE_SUPPORT.md`.

## Known issues

- The name `seam` collides with existing packages and with another company's
  CLI binary. See ADR 0002.
