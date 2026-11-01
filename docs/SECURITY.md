# Security and privacy

SEAM reads source code that may be confidential. Privacy is a property of the
architecture, not a promise in this file, so each guarantee below names the
mechanism that enforces it.

| Guarantee | Mechanism |
|---|---|
| No network access during analysis | The engine crates have no HTTP client dependency. `cargo tree` on `seam-cli` shows none. |
| No execution of analysed code | Adapters parse with tree-sitter and a YAML/JSON reader. No interpreter, compiler, package manager, build script or language server is invoked. |
| No telemetry | There is no telemetry code. |
| No account | There is no login or remote service. |
| No automatic Git writes | SEAM never commits, pushes or changes refs. Git support, when added, opens repositories read-only. |
| Bounded file access | Discovery starts at the given root, does not follow symlinks, honours `.gitignore`, skips the `.git` directory and files over 2 MiB. |
| Malformed input is safe | Parsers are error-tolerant; extraction skips error nodes and records a diagnostic. Adapters have no `unsafe` code and do not index slices with unchecked offsets taken from input. |
| Local storage only | Results go to `<repo>/.seam/seam.db` or the path given to `--store`. |

## Secrets

SEAM stores short excerpts of the source it cites. To avoid copying secrets
into its database:

- files that conventionally hold secrets are excluded from discovery by name:
  `.env` and `.env.*` other than `.env.example`, `*.pem`, `*.key`, `*.p12`,
  `*.pfx`, `id_rsa*`, `id_ed25519*`, `.npmrc`, `.pypirc`;
- configuration Seams (planned) will record variable names, never values.

The exclusion list is in `seam-analysis` and covered by tests.

## Reporting a vulnerability

Until there is a public release, report problems privately to the repository
owner rather than in a public issue.
