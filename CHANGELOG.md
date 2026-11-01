# Changelog

## 0.1.0 (2026-11-01)

The first public release: HTTP Seams between TypeScript or JavaScript
clients and Python FastAPI services, proven from source.

- `seam scan`, `seam seams`, `seam explain`, `seam report` and
  `seam languages`, with English and Persian output.
- TypeScript and JavaScript clients: `fetch` and `axios` requests, followed
  through wrapper functions (including wrappers of wrappers), constants,
  relative imports and `import.meta.env` or `process.env` with fallbacks.
- FastAPI services: routes, router prefixes and `include_router` across
  modules, Pydantic models with aliases and inheritance, request bodies.
- OpenAPI 3.0 and 3.1 operations attached to the routes they describe.
- Docker Compose services and environment used to resolve base URLs to
  the component that serves them.
- Field-by-field comparison of responses and request bodies, with
  missing-field, type-mismatch, nullable-mismatch, contract-drift,
  no-provider and untested findings.
- Evidence classes (VERIFIED, INFERRED, PARSED, UNKNOWN), confidence with
  reasons, written assumptions, and an Evidence Passport per Seam that
  re-checks the cited files.
- A local SQLite ledger with lifecycle states across scans.
- JSON (`seam-report/1`) and Markdown reports.
- Binaries for Windows, macOS (Apple silicon and Intel) and Linux (x64 and
  ARM64), and a container image.

Not in this release: SQL, environment-variable and Rust Seams, Git history
and Change Rehearsal, file watching, the desktop application, code signing.
