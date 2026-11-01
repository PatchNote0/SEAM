# Project status

Updated: 2026-11-01, release 0.1.0. This page says what exists and what does not. If it
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
| `seam-cli` | `seam scan`, `seams`, `explain`, `report` and `languages`; English and Persian output (`--lang`, `SEAM_LANG`, `LANG`); JSON report `seam-report/1` and Markdown reports, right to left in Persian; evidence revalidation with stale marking; `--fail-on`; distinct exit statuses; Ctrl-C cancellation; colour only on terminals. | `cargo test -p seam-cli`: 9 passed, running the built binary against a copy of the fixture. |
| Continuous integration | rustfmt, clippy with `-D warnings`, tests on Linux, macOS and Windows, a build with the minimum Rust version (1.90), the fixture's own pytest, tsc and Vitest suites. | `.github/workflows/ci.yml` |
| Release | Tag-triggered builds for Windows x64, macOS arm64 and x64, Linux x64 and ARM64 (static musl), archives with `SHA256SUMS`, a GitHub Release with notes from `CHANGELOG.md`, and a container image on GitHub Container Registry. | `.github/workflows/release.yml`, `Dockerfile` |
| Documentation | README in English and Persian, `docs/CLI.md`, changelog. | |

## Not built yet

- Fixture defects 3 (renamed SQL column) and 4 (missing environment
  variable): they need SQL and environment-variable Seams.
- Fixture defect 6 (scripted Git history) and Change Rehearsal.
- Rust (Axum) producers and Python HTTP clients as consumers.
- Git integration, file watching, incremental re-extraction, the desktop
  application.
- Code signing and notarisation of the release binaries.

## Known issues

- The name `seam` collides with existing packages and with another company's
  CLI binary. See ADR 0002.
- Help text (`--help`) is English only.
- Terminals differ in right-to-left support; Persian lines can look reversed
  in some of them. The Persian Markdown report is the reliable reading.
