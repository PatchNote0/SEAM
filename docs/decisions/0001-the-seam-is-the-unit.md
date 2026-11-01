# 0001. The Seam is the unit of analysis

Status: accepted, 2026-11-01

## Context

Tools that map dependencies usually store a graph: nodes for files or
symbols, edges for references. Cross-language boundaries do not fit that
model well. The link between a TypeScript call and a Python handler is not a
symbol reference; it is a route string, a method, two type declarations, an
optional contract and a set of assumptions about how requests are routed.

## Decision

The stored and displayed unit is the **Seam**: a boundary object with its own
identity, participants, mechanism, contract, field mappings, findings,
evidence, classification, confidence and lifecycle. Graphs are derived views
built from Seams when a view needs one.

Crates follow responsibilities rather than the proposed folder list:
`seam-core` (model and pure rules), `seam-adapters` (per-file extraction),
`seam-analysis` (discovery and linking), `seam-storage` (SQLite) and
`seam-cli`. Shapes, compatibility and evidence are model concerns and live in
`seam-core` instead of separate `seam-contracts` and `seam-evidence` crates.

## Alternatives considered

- A property graph as the primary store. Rejected: every query the product
  needs (the ledger, a passport, a lifecycle diff) is about one boundary at a
  time, and edges would have to carry most of a Seam anyway.
- One crate per concern from the brief. Rejected for now: several would hold
  one or two files and add dependency plumbing without separating anything.

## Consequences

- The ledger, passport and lifecycle are simple queries over one table.
- Change Rehearsal needs traversal across Seams that share fields or
  components. A graph view will be built for it when it is implemented.
