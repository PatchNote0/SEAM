# 0004. One local SQLite database per repository

Status: accepted, 2026-11-01

## Context

Snapshots, evidence and lifecycle history need durable storage that the CLI
and the desktop application can share, with no server.

## Decision

SQLite through `rusqlite` with the bundled SQLite build, one database per
repository at `<repo>/.seam/seam.db` by default. Migrations are numbered SQL
applied in a transaction, tracked in `PRAGMA user_version`. Each Seam
observation stores its full JSON next to relational columns used for
filtering.

## Alternatives considered

- An embedded graph database. Rejected: no query here needs graph semantics
  yet, and SQLite is easier to inspect, back up and migrate.
- Plain JSON files per snapshot. Rejected: lifecycle queries across
  snapshots and filtering the ledger would mean loading everything.

## Consequences

- The `.seam/` folder is created inside the analysed repository unless
  `--store` points elsewhere. SEAM never scans it.
- Bundling SQLite adds compile time but removes a system dependency.
