# Product differentiation

Research date: 2026-11-01. Claims about other products come from their own
documentation or from secondary sources, marked as such. Where a fact could
not be confirmed it says so. Nothing here claims SEAM is the first or only
tool of its kind.

## The question SEAM answers

> What connects these independently built parts of the system, what proves
> that it connects, and what changes when the connection breaks?

Most tooling answers one slice of that question, inside one language, one
artefact type or one runtime. SEAM's subject is the boundary itself.

## What already exists

| Category | Representative tools | What they do well | What they do not try to do |
|---|---|---|---|
| Spec-to-spec diffing | oasdiff (OpenAPI), Buf breaking (Protobuf), GraphQL Inspector | Classify the difference between two versions of one contract; oasdiff grades findings as ERR, WARN and INFO and gates CI with `--fail-on`. Buf compares a schema against any earlier input, including a Git ref. | Look at the code on either side of the contract. A spec can be internally consistent and still disagree with the implementation and its clients. |
| Consumer-driven contract testing | Pact, Specmatic | Pact turns consumer expectations into contract files that the provider verifies in CI, with a broker and `can-i-deploy` gate (secondary sources). Specmatic turns specifications into executable contract tests. | Work without someone writing and maintaining the contract tests. Discover boundaries nobody has written a test for yet. |
| Runtime API discovery | Traffic-based tools (API observability products) | See what actually crosses the wire. | Work offline, before deployment, or on code paths with no traffic. |
| Declared service catalogs | Backstage | `providesApis` and `consumesApis` relations are generated from what teams declare in `catalog-info.yaml` descriptors. | Check that the declaration matches the code. |
| Code intelligence and navigation | Sourcegraph with SCIP indexers, Kythe, Glean | Precise go-to-definition and references across repositories when SCIP indexes exist for both sides; SCIP indexers exist for Go, Java, TypeScript, Python, Ruby, C/C++, C# and more. | Treat an HTTP route string or an SQL column name as a reference between languages. Cross-language links at a network or database boundary are not symbol references. |
| Code property graphs and SAST | CodeQL, Joern, Semgrep | Deep, query-based analysis within a language; Joern merges language-specific frontends into one code property graph. A 2026 paper on Ant Group's YASA notes that CodeQL's per-language extractors leave little of its code language-agnostic, and a survey reports incomplete support for taint across language boundaries. | Model cross-service contracts as first-class objects with lifecycle and evidence. |
| Behavioural code analysis | CodeScene, code-maat | Change coupling from commits, author proximity and shared ticket IDs, across repositories, at file and function level; hotspots from activity. CodeScene's own docs say coupling is neither good nor bad in itself. | Prove a dependency. Co-change is correlation. |
| Data lineage | DataHub's SQL parser (built on SQLGlot), sqllineage, OpenLineage/Marquez | Table and column lineage from SQL statically or from runtime events. | Follow the column into the application model, the API serializer and the frontend type that reads it. |
| Migration safety | Squawk, Atlas migrate lint (from prior knowledge, not re-verified here) | Flag unsafe migration operations. | Find which application queries and models a migration breaks. |
| Test impact analysis | Teamscale (TIA), Launchable and Gradle (predictive test selection), Nx/Bazel affected graphs | Choose tests from coverage maps, build graphs or models trained on history. Launchable says its predictive selection does not use static analysis. | Explain which contract a test protects. |
| Code explorers | Sourcetrail (archived), SciTools Understand | Interactive symbol graphs inside a codebase. | Cross-service boundaries and their contracts. |

## Underserved workflows

1. **"Who reads this field?" across languages.** Renaming a response field or
   a column means grepping three languages and hoping. The spec diff tools see
   the contract, code intelligence sees one language, lineage sees SQL. Nobody
   joins them with the evidence attached.
2. **Contract drift where no contract test exists.** Pact needs tests that
   someone wrote. Many teams have an OpenAPI file, a backend and a frontend
   that each drifted on their own.
3. **Reviewing a change at the boundary level.** A pull request changes a
   Pydantic model. Reviewers want to know which clients and tests sit on the
   other side before merging, without deploying anything.
4. **Honest uncertainty.** Static tools tend to either over-report (keyword
   matches presented as dependencies) or stay silent. Developers need to see
   what was proven, what was inferred and what could not be resolved.

## What SEAM does differently

- **The Seam is the unit, not the edge.** A Seam carries its producer,
  consumer, mechanism, contract, field mappings, findings, evidence,
  classification and lifecycle. It is inspected on its own, listed in a
  ledger and tracked across snapshots. A graph is one view of it, not the
  storage model.
- **Both sides from source, contract optional.** SEAM extracts the client
  call and the server route from code, then links a declared contract when
  one exists. It does not require a spec, tests or traffic.
- **Evidence classes instead of scores.** Every Seam is VERIFIED, PARSED,
  INFERRED, CORRELATED or UNKNOWN with the reasons and assumptions spelled
  out. No invented percentages. Co-change history can only ever produce
  CORRELATED.
- **Adapters declare what they can resolve.** A TypeScript adapter that does
  not follow `tsconfig` path aliases says so in its capability declaration,
  and the Evidence Passport records the assumption when it matters.
- **Change Rehearsal (planned).** Compare two revisions and report the Seams
  whose contract, producer or consumer changed, split into confirmed and
  possible consequences, with the tests on each side.
- **Local-first by construction.** No account, no upload, no model calls and
  no execution of the analysed code.

## What SEAM deliberately does not do

- Runtime tracing, traffic capture or APM.
- Security scanning or taint analysis. Tools like CodeQL, Joern and Semgrep
  are better at it.
- General code search and symbol navigation inside one language.
- Contract test execution. SEAM points at the tests that cover a Seam and
  suggests missing ones; it does not run them.
- Narrative summaries. Every sentence in an Evidence Passport is derived
  from extracted facts.
- A hosted service or team dashboard.
- Service catalog management. SEAM can later read Backstage descriptors as
  declarations to check, not replace them.

## Naming check

Checked on 2026-11-01. This is a collision check, not a trademark search;
trademark availability has not been assessed.

| Where | Name | Status |
|---|---|---|
| Company | Seam (seam.co) | An access-control and smart-lock API company. Its CLI installs as `seam` via its Homebrew tap and as `seam-cli` on npm. |
| crates.io | `seam` | Taken (Symbolic Expressions As Markup). |
| crates.io | `seam-core` | Taken (a validation engine, updated September 2026). |
| crates.io | `seam-cli`, `seam-engine` | Taken (SeamJS framework). |
| crates.io | `seamctl`, `seam-contracts` | Free at the time of checking. |
| npm | `seam`, `seam-cli` | Taken by Seam (seam.co). |
| PyPI | `seam` | Taken. |
| Java | JBoss Seam | A historical Java framework (prior knowledge). |

Consequences, recorded in [ADR 0002](decisions/0002-naming-and-package-names.md):
the product name stays SEAM for now, workspace crates are marked
`publish = false`, and the `seam` binary may collide with Seam's own CLI on
machines that have both. A distinct distribution name should be chosen before
any public package release.

## Sources

- oasdiff, Pact and Specmatic: practitioner summaries collected at
  [tessl.io openapi-contract-diff](https://tessl.io/registry/testland/openapi-contract-diff)
  and [GitHub topic: api-contract-testing](https://github.com/topics/api-contract-testing) (secondary).
- [Buf: breaking change detection](https://buf.build/docs/breaking-overview)
- [Backstage: well-known relations](https://backstage.io/docs/features/software-catalog/well-known-relations)
- [Sourcegraph: cross-repository code navigation](https://sourcegraph.com/blog/cross-repository-code-navigation),
  [precise code navigation (legacy docs)](https://docs-legacy.sourcegraph.com/user/code_intelligence/lsif)
- [CodeScene: change coupling](https://codescene.io/docs/guides/technical/change-coupling.html),
  [terminology](https://codescene.io/docs/terminology/codescene-terminology.html)
- [DataHub: extracting column-level lineage from SQL](https://datahub.com/?p=4506)
- [YASA: multi-language taint analysis at Ant Group](https://arxiv.org/pdf/2601.17390),
  [survey of program analysis approaches](https://arxiv.org/pdf/2604.27000),
  [Joern documentation](https://docs.joern.io/)
- [CloudBees: predictive test selection vs test impact analysis](https://www.cloudbees.com/blog/predictive-test-selection-vs-test-impact-analysis),
  [TUM thesis comparing TIA approaches](https://teamscale.com/hubfs/Publications/2023%20Thesis%20-%20Malek%20Ben%20Slimane%20-%20Comparatice%20Analysis%20of%20Different%20Approaches%20for%20Test%20Impact%20Analyses%20for%20Real%20World%20Test%20Suites.pdf)
- [Seam CLI documentation](https://docs.seam.co/developer-tools/seam-cli)
- crates.io, npm and PyPI registry APIs, queried directly.
