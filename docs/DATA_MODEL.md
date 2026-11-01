# Data model

The model lives in `crates/seam-core` and is persisted by `crates/seam-storage`.
This document is the contract between them; when the two disagree, the code
is wrong or this file is stale, and both are bugs.

## The Seam

A **Seam** is a boundary where two independently built components meet
through an agreed contract. It is the unit SEAM discovers, stores, explains
and tracks. It is not a graph edge: an edge says *A relates to B*, a Seam
says *how*, *through what contract*, *with which fields*, *proven by what*,
*with what certainty* and *since when*.

| Field | Meaning |
|---|---|
| `id` | Stable identifier, `sm_` followed by 20 base32 characters of a BLAKE3 hash of the Seam key. Line numbers are not part of it, so moving code does not change identity. |
| `key` | Canonical identity: mechanism, normalised contract key, producer component, consumer component. For HTTP: `http GET /accounts/{} producer=api consumer=web`. |
| `mechanism` | How the components talk. Implemented: `Http { method, route }`. Planned: SQL table or column, environment variable, GraphQL operation, event channel, serialised message. |
| `producer` | The side that implements the interface: component, symbol (for example the handler function) and the evidence of its declaration. `None` when no implementation was found. |
| `consumer` | The side that depends on it: component, symbol and call sites. |
| `contract` | The declared contract when one exists, for example an OpenAPI operation with its document path and operation id. |
| `mappings` | One row per field name, holding what the consumer expects, what the producer sends and what the contract declares. |
| `findings` | Compatibility problems derived from the mappings and the linking. |
| `classification` | The kind of evidence that establishes the Seam (below). |
| `confidence` | A category with reasons, never a percentage (below). |
| `assumptions` | Every assumption the analysis had to make, in words, each tied to the evidence that made it necessary. |
| `evidence` | Evidence records (below). Every claim in an Evidence Passport points to one. |
| `tests` | Tests that exercise either side of the Seam, with evidence. |
| `state` | Lifecycle state relative to the previous snapshot. |
| `snapshot` | The snapshot it was observed in, with its analysis time. |

## Participants and components

A **component** is an independently built unit: a directory with its own
manifest (`package.json`, `pyproject.toml`, `Cargo.toml`, `go.mod`). Its name
comes from the manifest, falling back to the directory name. Docker Compose
services that build from a component's directory become that component's
network aliases, so a base URL of `http://api:8000` can be resolved to the
component that serves it.

## Evidence classification

Every Seam carries exactly one class. The class describes the *weakest link*
in the chain that establishes it.

| Class | Meaning | Example |
|---|---|---|
| `VERIFIED` | Every link is a literal fact or a deterministic resolution, and no assumption was needed. | `fetch` with a literal path, through a wrapper whose parameter flows straight into the URL, a base URL read from Compose that names the service, a FastAPI route with a literal path, one unique match. |
| `PARSED` | One side was extracted from explicit syntax but its counterpart was not found, so the relationship is not proven. | A client call to `/v1/users/{}` when no analysed service declares that route. |
| `INFERRED` | The match needed at least one documented assumption. | The base URL comes from an environment variable whose value is not in the repository, so the match relies on the route being unique among analysed services. |
| `CORRELATED` | Only history links the two sides, through co-change. Never a code dependency. | Reserved for the Git analysis in a later phase. |
| `UNKNOWN` | There is not enough information to describe the boundary. | A URL built from runtime data. The call is recorded so that it is visible, not silently dropped. |

**Resolution is not assumption.** Following an import, propagating a
constant, composing a router prefix or forwarding a parameter through a
wrapper are deterministic steps over code that was parsed. They are recorded
as evidence with the role `resolution` and do not lower the class.
Assumptions do.

## Confidence

Confidence answers a different question from classification: *how complete
and unambiguous is the chain?*

| Level | When |
|---|---|
| `strong` | No assumptions, a single candidate on each side. |
| `moderate` | Assumptions were needed, but the candidate is unique. |
| `weak` | Several candidates matched, a side is missing, or the shapes could not be resolved. |

The level always comes with its reasons, each a sentence derived from the
analysis, for example "matched 1 of 2 services exposing GET /accounts/{}".

## Shapes and field mappings

Types from every language are lowered to one structural model, the **shape**:
`string`, `integer`, `number`, `boolean`, `null`, `array(shape)`,
`object(fields)`, `union(shapes)`, `any` and `unknown(reason)`. An object field
records its JSON name (after aliases), its declared name, whether it is
required, whether it is nullable, and the source span that declared it.

Lowering rules worth knowing:

- TypeScript `field?: T` is optional; `T | null` is nullable; `T[]` and
  `Array<T>` are arrays; `number` covers integer and number.
- Python `Optional[T]` and `T | None` are nullable; `list[T]` and `List[T]`
  are arrays; Pydantic `Field(alias=...)` and `serialization_alias=...` change
  the JSON name; a field with a default is still present in a response.
- OpenAPI `required` lists decide requiredness; `nullable: true` and
  `type: [T, "null"]` decide nullability; local `$ref`s are resolved.

A **field mapping** joins the three views of one JSON field by name: what the
consumer reads, what the producer sends, and what the contract declares.

## Findings

| Finding | Raised when | Severity |
|---|---|---|
| `missing-field` | The consumer requires a field the producer never sends. | breaking |
| `type-mismatch` | Both sides have the field with incompatible shapes. | breaking |
| `nullable-mismatch` | The producer may send `null` where the consumer's type does not allow it. | risk |
| `contract-drift` | The contract and the implementation disagree about a field. | risk |
| `no-provider` | A consumer calls a route no analysed service declares. | risk |
| `untested` | No test references either side of the Seam. | info |

Findings are statements about static code. A `missing-field` finding means
the consumer will read `undefined`; whether that breaks the product depends
on how the value is used, which SEAM does not claim to know.

## Lifecycle

Lifecycle compares a Seam with its state in the previous snapshot of the
same repository.

| State | Meaning |
|---|---|
| `discovered` | Not present in the previous snapshot. |
| `reconfirmed` | Present before and re-established from current sources without change. Called "verified" in the original brief; renamed so it is never confused with the `VERIFIED` evidence class. |
| `changed` | Present before, but its contract fingerprint (mechanism, shapes, contract operation) differs. |
| `potentially-incompatible` | Has at least one `breaking` or `risk` finding. |
| `unresolved` | A side is missing or the boundary is `UNKNOWN`. |
| `removed` | Present in the previous snapshot, absent now. Kept as history, never shown as active. |

When several apply, the first in this order wins: removed,
potentially-incompatible, unresolved, changed, discovered, reconfirmed.

## Evidence records

| Field | Meaning |
|---|---|
| `role` | `consumer-call`, `producer-route`, `contract-operation`, `consumer-type`, `producer-model`, `contract-schema`, `resolution`, `assumption`, `test`. |
| `span` | Repository-relative path with `/` separators, start and end as 1-based line and 1-based byte column, and the byte range. |
| `excerpt` | The source lines the span covers, at most 12, taken from the file at analysis time. |
| `file_hash` | BLAKE3 of the whole file at analysis time. |
| `method` | The adapter, its version and the technique, for example `typescript@0.1 call-expression:fetch`. |
| `note` | One sentence that says what this record proves. |

**Revalidation.** `seam explain` re-hashes every file an Evidence Passport
cites. If a hash differs, the record is marked stale, and the Seam is shown
as stale until the next scan, whatever its stored class. A stale Seam is
never displayed as `VERIFIED`.

## Persistence

SQLite, one database per analysed repository, at `<repo>/.seam/seam.db` unless
`--store` says otherwise. Migrations are numbered and applied in order inside a
transaction; the schema version is kept in `PRAGMA user_version`.

| Table | Holds |
|---|---|
| `snapshot` | One row per scan: root, Git head when known, engine version, start time, duration, file count. |
| `file` | Every analysed file per snapshot: path, BLAKE3 hash, language, size. |
| `seam` | One row per Seam id ever seen: key, mechanism, first and last snapshot, current state. |
| `observation` | One row per Seam per snapshot: class, confidence, contract fingerprint, and the full Seam as JSON for faithful replay. |
| `evidence` | Evidence records with their spans, excerpts and hashes, indexed by path for invalidation. |
| `finding` | Findings per Seam per snapshot. |

The JSON in `observation` is the source of truth for `seam explain`; the
relational columns exist so the ledger can be filtered and sorted without
parsing JSON.
