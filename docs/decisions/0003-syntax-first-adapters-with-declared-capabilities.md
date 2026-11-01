# 0003. Syntax-first adapters with declared capabilities

Status: accepted, 2026-11-01

## Context

Precise semantics come from compiler front ends: the TypeScript compiler API,
rust-analyzer, Go's type checker. Running them means a Node runtime, project
configuration, installed dependencies and sometimes build scripts. SEAM must
work on a fresh checkout, offline, without executing project code.

## Decision

Adapters start with tree-sitter, which parses incomplete and malformed code
without executing anything. Cross-file resolution (relative imports, Python
module paths, router prefixes, constants) is implemented in SEAM's linking
stage over the extracted facts. Every adapter publishes what it can and cannot
resolve, and anything it could not resolve becomes an explicit assumption on
the Seam that depended on it.

Compiler-backed resolution is added per language where tree-sitter cannot
answer the question, for example TypeScript type aliases defined through
mapped or conditional types. It will be optional and off by default, because
it requires toolchains on the user's machine.

## Alternatives considered

- Language servers for everything. Rejected as the default: heavy, slow to
  start, need dependencies installed, and some evaluate project configuration.
- SCIP indexes as the input. Kept as a future optional source: they give
  precise symbols but say nothing about routes, columns or environment names.

## Consequences

- SEAM can analyse a repository it cannot build.
- Some type questions cannot be answered by syntax alone; those surface as
  `unknown` shapes with a reason rather than guesses.
