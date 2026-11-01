<div align="center">

**English** · [فارسی](README.fa.md)

</div>

# SEAM

Evidence-first cross-language contract intelligence.

SEAM finds the boundaries between independently built parts of a system, such
as a TypeScript client calling a FastAPI service that an OpenAPI document
describes and Docker Compose wires up. It records each boundary as a **Seam**:
who calls whom, through which route, with which fields, proven by which lines
of code, and how sure SEAM is.

```text
$ seam scan
SEAM 0.1.0 · scanned ~/ledgerline in 4 ms
25 files · 4 components · 4 routes · 5 Seams

GET /accounts/{account_id}  VERIFIED
  @ledgerline/web (fetchAccount) → ledgerline-api (get_account)
  sm_hajbrsw7xvh8h76etgjg · potentially-incompatible · contract getAccount
  breaking  The consumer reads `name`; the producer's response has no field `name`.
```

That finding comes from the [polyglot fixture](fixtures/polyglot/FIXTURE.md):
the API renamed `name` to `display_name`, the client still reads `name`, and
the client's own tests pass because they mock the old response.

## Install

Download the archive for your system from the
[latest release](https://github.com/PatchNote0/seam/releases/latest), unpack
it and put `seam` on your `PATH`.

| System | Archive |
|---|---|
| Windows, x64 | `seam-<version>-x86_64-pc-windows-msvc.zip` |
| macOS, Apple silicon | `seam-<version>-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `seam-<version>-x86_64-apple-darwin.tar.gz` |
| Linux, x64 (static) | `seam-<version>-x86_64-unknown-linux-musl.tar.gz` |
| Linux, ARM64 (static) | `seam-<version>-aarch64-unknown-linux-musl.tar.gz` |

Every release has a `SHA256SUMS` file. The binaries are not code-signed yet.
On macOS, clear the download quarantine with
`xattr -d com.apple.quarantine ./seam`. On Windows, SmartScreen may ask you to
confirm the first run.

The same binary ships as a container image:

```sh
docker run --rm -v "$PWD:/repo" ghcr.io/patchnote0/seam scan /repo --no-store
```

To build from source you need Rust 1.90 or newer and a C compiler (the
tree-sitter grammars and SQLite are compiled in):

```sh
cargo install --locked --git https://github.com/PatchNote0/seam seam-cli
```

## Use

| Command | What it does |
|---|---|
| `seam scan [PATH]` | Scans the repository, records a snapshot in `PATH/.seam/seam.db` and prints the Seams with their findings. |
| `seam seams [PATH]` | The ledger: every Seam from the last scan with its lifecycle state, then the ones removed since. |
| `seam explain ID` | The Evidence Passport of one Seam: why SEAM believes it, every field compared, assumptions, tests, and the source lines behind each claim, re-checked against the files as they are now. |
| `seam report [PATH]` | Scans without recording and writes a JSON report (`seam-report/1`) or a Markdown report. |
| `seam languages` | What each adapter reads, follows across files, or does not handle. |

`seam scan --fail-on breaking` exits with status 1 when a breaking finding
exists, for use in CI. All commands, options, output formats and exit
statuses are in [docs/CLI.md](docs/CLI.md).

## What it reads

| Side | Read |
|---|---|
| Clients | TypeScript and JavaScript: `fetch` and `axios` calls, followed through wrapper functions, constants, imports and `import.meta.env` or `process.env` with their fallbacks. |
| Services | Python FastAPI: routes, router prefixes and `include_router` across modules, Pydantic models with aliases and inheritance. |
| Contracts | OpenAPI 3.0 and 3.1 in YAML or JSON, with local `$ref`. |
| Wiring | Docker Compose: which service builds which folder, and the environment it sets. |
| Tests | Vitest or Jest `it`/`test` blocks and FastAPI `TestClient` requests. |

`seam languages` prints the full capability list, including what is not
handled. See [docs/LANGUAGE_SUPPORT.md](docs/LANGUAGE_SUPPORT.md).

## How sure it is

Every Seam has one class, set by the weakest link in its chain of evidence.

| Class | Meaning |
|---|---|
| `VERIFIED` | Every link is a literal fact or a deterministic resolution, with no assumption. |
| `INFERRED` | The match needed an assumption, which is written down with the code that forced it. |
| `PARSED` | One side was read from source, but its counterpart was not found. |
| `UNKNOWN` | The target is computed at run time; the call is recorded instead of guessed. |

Confidence is `strong`, `moderate` or `weak`, always with its reasons, never a
percentage. See [docs/DATA_MODEL.md](docs/DATA_MODEL.md).

## Privacy

SEAM reads files and writes one local database. It opens no network
connection, runs nothing from the analysed repository, has no telemetry and
needs no account. Files that usually hold secrets (`.env`, keys, `.npmrc`) are
never read, and the scan output names them as skipped. Details in
[docs/SECURITY.md](docs/SECURITY.md).

## Persian

Every human-readable output is available in Persian: pass `--lang fa`, or set
`SEAM_LANG=fa`. `seam report --format markdown --lang fa` writes a
right-to-left report that reads well in any Markdown viewer. JSON output is
always English. See [README.fa.md](README.fa.md).

## Status

Version 0.1.0 is the first public release. It covers HTTP Seams between
TypeScript or JavaScript clients and FastAPI services. SQL, environment
variable and Rust (Axum) Seams, Git history and Change Rehearsal, file
watching and the desktop application are planned and not built.
[docs/PROJECT_STATUS.md](docs/PROJECT_STATUS.md) lists exactly what works and
how it was tested.

## Documents

- [The `seam` command](docs/CLI.md)
- [Language support](docs/LANGUAGE_SUPPORT.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Data model](docs/DATA_MODEL.md)
- [Security and privacy](docs/SECURITY.md)
- [Product differentiation](docs/PRODUCT_DIFFERENTIATION.md)
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Decisions](docs/decisions/)
- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT](LICENSE-MIT), at your option.
