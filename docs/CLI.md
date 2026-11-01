# The `seam` command

`seam` scans a repository, records what it finds in a local ledger and
explains each Seam from its evidence. It reads files and nothing else.

## Commands

### `seam scan [PATH]`

Scans `PATH` (default: the current folder), records a snapshot and prints
every Seam with its class, its lifecycle state and its findings, then the
routes no analysed client calls, the skipped files and any file that could
only be partly read.

| Option | Meaning |
|---|---|
| `--store FILE` | Record in `FILE` instead of `PATH/.seam/seam.db`. |
| `--no-store` | Do not record anything. |
| `--exclude PATTERN` | Leave out paths that match, written like a `.gitignore` line. Repeatable. |
| `--fail-on never\|breaking\|risk\|info` | Exit with status 1 when a finding of that severity or worse exists. Default `never`. |
| `--format text\|json` | `json` prints the report described below, with the snapshot it was recorded as. |
| `--threads N` | Worker threads. Default: one per core. |

Pressing Ctrl-C cancels the scan; nothing is recorded.

### `seam seams [PATH]`

The ledger of the last recorded scan: every Seam with its class, state,
confidence and finding counts, then the Seams removed since earlier scans.
`--class VERIFIED` and `--state potentially-incompatible` filter it; both
repeat. `--format json` prints the rows.

### `seam explain ID`

The Evidence Passport of one Seam. `ID` is the full id or enough of its start
to be unique, with or without the `sm_` prefix. It shows the participants,
the contract, the class, confidence and state, the reasons, every finding,
the field-by-field comparison of the response and the request body, the
assumptions, the tests on each side, and every evidence record with its file,
line and source excerpt.

Every cited file is hashed again. If one changed since the scan, its records
are marked stale and the Seam shows as `STALE` instead of its class until the
next `seam scan`. `--repo PATH` says where the store is (default: the current
folder); `--format json` prints the Seam, its state and the stale records.

### `seam report [PATH]`

Scans without recording and writes a report. `--format json` (the default)
or `--format markdown`; `--output FILE` writes to a file. When a store
exists, the report includes each Seam's lifecycle state against the last
recorded scan.

### `seam languages`

What each adapter reads from a file, what the linker follows across files,
and what is not handled. Anything not handled becomes an assumption or an
`UNKNOWN` Seam rather than a guess.

## Global options

| Option | Meaning |
|---|---|
| `--lang en\|fa` | Language of human-readable output. Without it, `SEAM_LANG` decides, then `LC_ALL` and `LANG` when they name Persian (`fa`). |
| `--color auto\|always\|never` | `auto` colours a terminal unless `NO_COLOR` is set. On Windows, `auto` means no colour, because older consoles print the escape codes. |

## Exit status

| Status | Meaning |
|---|---|
| 0 | Success. |
| 1 | `--fail-on` found a finding at or above its level. |
| 2 | The command line or an `--exclude` pattern is invalid. |
| 3 | The repository could not be read, or a report could not be written. |
| 4 | The store could not be opened or written, or was written by a newer SEAM. |
| 5 | Nothing is recorded yet, or no Seam has that id. |
| 130 | The scan was cancelled. |

## Reports

The JSON report has `"schema": "seam-report/1"`. The schema name changes
when a field is removed or changes meaning; new fields may appear without a
change. Top-level fields:

| Field | Content |
|---|---|
| `engine_version`, `generated_at`, `root`, `duration_ms` | Where and when the scan ran. |
| `summary` | File, component, route and Seam counts, Seams per class, findings per severity. |
| `components` | Name, folder, manifest and file count of each component. |
| `routes` | Every producer route and how many Seams reach it. |
| `seams` | Every Seam as described in [DATA_MODEL.md](DATA_MODEL.md), with its lifecycle `state` when a store was compared. |
| `skipped`, `diagnostics` | Files not read and why; files only partly read. |
| `recorded` | `scan --format json` only: the store and snapshot it was recorded in. |

Two scans of the same files give the same report apart from `generated_at`,
`duration_ms` and `recorded`. JSON is always in English.

The Markdown report has the same content for people. With `--lang fa` it is
written right to left, with source excerpts kept left to right.

## Persian output

Labels, classes, states, findings, reasons, assumptions and evidence notes
are all available in Persian. Terminals differ in how they show
right-to-left text; when Persian lines look reversed, use
`seam report --format markdown --lang fa --output report.md` and open the file
in a Markdown viewer. Help text (`--help`) is English only.

## Files SEAM writes

Only the store, `PATH/.seam/seam.db`, and a `.seam/.gitignore` that keeps the
folder out of Git; or the file given to `--store`, or to `report --output`.
`--no-store` writes nothing.
