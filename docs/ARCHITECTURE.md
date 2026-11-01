# Architecture

SEAM is a Rust analysis engine with two front ends: a CLI and, later, a
Tauri desktop application. Both call the same library; neither contains
analysis logic.

Status markers: **built** means implemented and tested on this branch;
**planned** means designed here but not yet built. `docs/PROJECT_STATUS.md`
is the authoritative list.

## Pipeline

```
 repository root
      │
      ▼
 discover        walk files, honour .gitignore and exclusions,
 (seam-analysis) skip oversized and binary files, hash with BLAKE3
      │
      ▼
 extract         one adapter per file kind, run in parallel with rayon;
 (seam-adapters) each returns facts with source spans and never panics
      │          on malformed input
      ▼
 link            resolve components, base URLs, router prefixes and
 (seam-analysis) wrappers; match consumers to producers; attach contracts
      │
      ▼
 judge           lower types to shapes, build field mappings, raise
 (seam-core)     findings, classify evidence, grade confidence
      │
      ▼
 persist         write a snapshot, compute lifecycle against the
 (seam-storage)  previous one, keep evidence for revalidation
      │
      ▼
 present         ledger, Evidence Passport, JSON report (seam-cli);
                 desktop views later
```

## Crates

| Crate | Responsibility | Depends on |
|---|---|---|
| `seam-core` | Domain model: Seam, participants, mechanisms, routes, shapes, field mappings, findings, evidence, classification, confidence, lifecycle. Route matching and shape compatibility, because both are pure rules about the model. No I/O. | serde, blake3 |
| `seam-adapters` | Per-file extraction. TypeScript and Python through tree-sitter, OpenAPI and Docker Compose through a spanned YAML/JSON reader built on tree-sitter. Each adapter publishes a capability declaration. | seam-core, tree-sitter and grammars |
| `seam-analysis` | Discovery, parallel extraction, cancellation, progress, component detection, cross-file resolution, linking and assembly of Seams. | seam-core, seam-adapters, ignore, rayon |
| `seam-storage` | SQLite schema, migrations, snapshots, lifecycle computation, ledger queries, evidence revalidation. | seam-core, rusqlite (bundled SQLite) |
| `seam-cli` | The `seam` binary: argument parsing, terminal output, JSON output, exit codes, Ctrl-C cancellation. | all of the above, clap |

The brief proposed `seam-parser`, `seam-contracts`, `seam-evidence`,
`seam-graph` and `seam-git` as separate crates. Today's split is narrower on
purpose: contracts and evidence are model types with rules attached and live
in `seam-core`; extraction is more than parsing, hence `seam-adapters`.
`seam-git` and graph traversal arrive with Change Rehearsal, when there is
code to put in them. See [ADR 0001](decisions/0001-the-seam-is-the-unit.md).

## Adapters

An adapter receives one file's path and bytes and returns `FileFacts`: the
declarations, calls, models and contracts found in it, each with a span.
Adapters do not see other files. Anything that needs more than one file, such
as following an import or composing a FastAPI router prefix declared in
`main.py`, happens in the linking stage, which has every file's facts.

Each adapter declares its capabilities as data, for example:

| Capability | TypeScript adapter |
|---|---|
| Relative imports | resolved |
| `tsconfig` path aliases | not resolved |
| Generic type arguments at call sites | read |
| Inferred return types | not resolved |

The declarations are printed by `seam languages` and quoted in an Evidence
Passport when a missing capability forced an assumption. See
[ADR 0003](decisions/0003-syntax-first-adapters-with-declared-capabilities.md)
for why tree-sitter comes first and compiler APIs later.

## Concurrency and cancellation

Extraction is CPU-bound and embarrassingly parallel, so it runs on rayon's
thread pool. Tree-sitter parsers are created per task because they are not
`Sync`. A cancellation token is checked between files; a cancelled scan
returns an error and writes nothing. There is no async runtime in the engine:
nothing in the current pipeline waits on the network or on timers. Tokio
will be introduced with the file watcher and the desktop bridge, where it
earns its place. See [ADR 0005](decisions/0005-no-async-runtime-in-the-engine-yet.md).

## Errors

Library crates define their own error types with `thiserror`. A malformed
file is not an error: tree-sitter recovers, the adapter skips error nodes and
the scan records a diagnostic with the file and the reason. Fatal errors are
the ones that make the result meaningless: an unreadable root, a database
that cannot be opened, a cancelled scan. The CLI turns errors into messages
that say what failed and what to do, and into distinct exit codes.

## Determinism

Facts, Seams and evidence are sorted by stable keys before they are reported
or stored. Two scans of the same tree produce byte-identical JSON reports
apart from timestamps and durations. Seam ids are hashes of keys, so they
survive line moves and reformatting.

## Privacy

SEAM reads files and writes one local database. It does not open network
connections, does not execute anything from the analysed repository and has
no telemetry. Details in [SECURITY.md](SECURITY.md).

## Not built yet

- Incremental re-extraction using cached facts keyed by file hash (the hashes
  are already stored).
- File watching (`seam watch`).
- Git comparison and Change Rehearsal (`seam rehearse`), co-change history.
- SQL, GraphQL, Rust (Axum) and environment-variable Seams.
- The Tauri desktop application.
