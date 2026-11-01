# Project status

Updated: 2026-11-01. This page says what exists and what does not. If it
disagrees with the code, the page is wrong and must be fixed.

## Built and tested

| Area | What exists | Evidence |
|---|---|---|
| Phase 0 documents | Product differentiation, architecture, data model, security, implementation plan, ADRs 0001 to 0006, development guides. | `docs/` |
| Polyglot fixture | Ledgerline: TypeScript web client, FastAPI API, Rust Axum ledger, Postgres migrations, OpenAPI, Compose. Defects 1, 2 and 5 are planted and documented; 3 and 4 are planted for later phases. | API pytest: 3 passed. Web: `tsc --noEmit` clean, Vitest 2 passed. Commands in `fixtures/polyglot/FIXTURE.md`. |
| `seam-core` | Route templates and matching, shapes, directional field comparison with findings, evidence and content hashes, the Seam model, stable ids, classification and lifecycle rules. | `cargo test -p seam-core`: 32 passed. Clippy with `-D warnings` and rustfmt clean. |
| `seam-adapters` | Per-file extraction with spans: TypeScript and JavaScript (requests through `fetch` and `axios`, wrapper candidates, URL constants and environment fallbacks, interfaces and type aliases as shapes, imports, test cases), Python (FastAPI apps, routers, routes, includes, Pydantic-style classes with aliases, imports, TestClient requests), OpenAPI, Docker Compose and manifests. File kinds by path, one `extract` entry point that never panics on malformed input, and declared capabilities. | `cargo test -p seam-adapters`: 36 passed. Clippy with `-D warnings` and rustfmt clean. |

## Not built yet

- `seam-analysis`: discovery, linking, Seam assembly.
- `seam-storage`: SQLite snapshots and lifecycle history.
- `seam-cli`: the `seam` command.
- Fixture defect 6 (scripted Git history).
- Change Rehearsal, Git integration, file watching, the desktop application.

No language is supported yet in the sense of `docs/LANGUAGE_SUPPORT.md`.

## Known issues

- The name `seam` collides with existing packages and with another company's
  CLI binary. See ADR 0002.
