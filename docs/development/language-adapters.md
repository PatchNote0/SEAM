# Adding a language adapter

What a new language or format adapter must deliver, from file discovery to its capability declaration and fixtures.

An adapter lives in `crates/seam-adapters/src/<language>.rs` and turns one file into `FileFacts`. Deliver all of the following in the same change.

1. **File discovery rules.** Extensions and file names it claims, added to `seam_adapters::kind_for_path`. Files it must not claim (generated, minified, vendored) and how they are recognised.
2. **Parser selection.** Tree-sitter grammar, or a reason to use something else (ADR 0003). Pin the grammar version.
3. **Symbol extraction.** Functions, types, constants and imports the linker needs, each with a `Span`.
4. **Contract extraction.** Routes, calls, models, schemas or queries, lowered to `seam-core` types and shapes.
5. **Reference resolution.** What it resolves itself (within the file) and what it hands to the linker (cross-file). Never resolve by guessing names.
6. **Evidence.** Every fact carries the span of the syntax that proves it. No fact without a span.
7. **Errors.** Malformed files produce facts for the valid parts and a diagnostic; they never panic or abort the scan.
8. **Capability declaration.** A `Capabilities` entry listing what is resolved, read, or not resolved. `seam languages` prints it; be specific and conservative.
9. **Unit tests.** Positive cases, negative look-alikes, and malformed input, in the adapter module.
10. **Integration fixture.** Files under `fixtures/languages/<language>/` and a test that scans them end to end.
11. **Documentation.** A row in `docs/LANGUAGE_SUPPORT.md` that matches the capability declaration exactly.

Do not mark a language as supported in any document until its tests pass.
