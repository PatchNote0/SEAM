# Implementation plan

Work proceeds in vertical slices. A phase is done when its features meet the
definition of done in `CONTRIBUTING.md`: implemented, tested, run, errors handled,
documented, limitations known. Progress is tracked in `PROJECT_STATUS.md`.

## Phase 0: research and specification

- [x] Survey the toolchain and the existing tools in this space.
- [x] Research adjacent tools: `PRODUCT_DIFFERENTIATION.md`.
- [x] Architecture, data model, evidence semantics, ADRs 0001 to 0006.
- [x] Development guides under `docs/development`.

## Phase 1: Rust foundation

Goal: scan a repository, hash its files, extract facts, store a snapshot.

1. Workspace with `seam-core`, `seam-adapters`, `seam-analysis`,
   `seam-storage`, `seam-cli`, all `publish = false`.
2. `seam-core` model types with serde, Seam id derivation, route templates.
   Tests: id stability under span changes; route normalisation.
3. Discovery in `seam-analysis`: `ignore`-based walk, secret-file exclusions,
   size limit, BLAKE3 fingerprints, language detection.
   Tests: `.gitignore` honoured; `.env` excluded but `.env.example` kept;
   oversized file skipped with a diagnostic; symlinks not followed.
4. `seam-storage`: migration 1, snapshot and file rows, Seam observations,
   evidence, findings, lifecycle computation, ledger query.
   Tests: migrations idempotent; lifecycle transitions across three snapshots.

## Phase 2: first working Seam

Goal: TypeScript client to FastAPI route to OpenAPI operation, with an
Evidence Passport, on `fixtures/polyglot`.

1. Spanned YAML/JSON reader over tree-sitter, with line and column for every
   key and value. Tests: block and flow styles, anchors reported as
   unsupported, malformed input.
2. OpenAPI adapter: operations, parameters, request and response schemas,
   local `$ref` resolution, shapes with spans.
3. Python adapter: FastAPI app and router objects, route decorators, router
   prefixes, `include_router` calls, handler parameters and return
   annotations, Pydantic models with aliases and inheritance.
4. TypeScript adapter: `fetch` calls, wrapper functions that forward a path
   parameter into `fetch`, method detection, template literals, generic type
   arguments, interfaces and type aliases with fields, imports and exports,
   constants and `import.meta.env` reads.
5. Docker Compose adapter: services, build contexts, environment entries.
6. Linking: components from manifests, Compose aliases, base URL resolution,
   wrapper resolution, router prefix composition, route matching with
   ambiguity handling, contract attachment.
7. Judgement: shapes, field mappings, findings, classification, confidence.
8. Tests on the fixture that assert the discovered Seams, their classes and
   their findings, plus negative tests: a string that merely looks like a
   route is not a call; a dynamic URL yields UNKNOWN; no keyword matching.

## Phase 3: contract intelligence

Test association, contract drift between OpenAPI and implementation, missing
contract detection, fact caching by file hash, false-positive review against
more fixtures.

## Phase 4: Rust and databases

Axum routes and Serde models, SQL migrations and queries, column-level Seams,
removed-column investigation.

## Phase 5: Change Rehearsal

Git revisions through `gix`, changed boundary detection, impact traversal,
confirmed versus possible consequences, suggested regression tests,
reproducible reports.

## Phase 6: CLI completion

`rehearse`, `watch`, report formats, documented exit codes.

## Phase 7: desktop application

Tauri 2, React, TypeScript, Vite. Overview, Seam Ledger, Evidence Inspector,
Contract Inspector, Change Rehearsal, graph exploration, settings.

## Phase 8: quality and language expansion

New adapters only after the first ones are reliable: Go, Java, Kotlin, C#,
then others when justified.

## Phase 9: release

Security review, benchmarks on documented hardware, packaging for macOS and
Windows, README with real screenshots, CI, release notes.
