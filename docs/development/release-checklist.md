# Release checklist

Steps to run, and record, before tagging a release.

Record the command and its result for each step in the release notes draft. A step that was not run is listed as not run.

1. **Build.** `cargo build --workspace --release` on macOS and Windows runners; desktop bundle builds where present.
2. **Tests.** `cargo test --workspace` with zero failures; fixture tests included; note the count.
3. **Static analysis.** `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all --check`; TypeScript `tsc --noEmit` for the desktop app.
4. **Security.** `cargo audit` or `cargo deny check advisories`; review `unsafe` (expected: none); confirm no network or process-spawning crates in the engine with `cargo tree`.
5. **Privacy.** Scan the fixture with networking disabled; confirm `.env` values never appear in `.seam/seam.db`.
6. **Performance.** Run the benchmarks; compare with the previous release; publish numbers only with hardware, repository size and method.
7. **Documentation.** `LANGUAGE_SUPPORT.md` matches `seam languages`; `PROJECT_STATUS.md` matches reality; README commands copy-paste and work.
8. **Packaging.** Binaries named and signed per platform; checksums generated.
9. **Integrity.** Tag points at the tested commit; release notes list known limitations; no fictional numbers anywhere.

## Publishing

Update the version in `Cargo.toml` and the `CHANGELOG.md` section for it,
then on GitHub open Releases, Draft a new release, create the tag `vX.Y.Z` on
the default branch and publish. `.github/workflows/release.yml` builds every
target, attaches the archives and `SHA256SUMS`, writes the notes from the
changelog when the release has none, and pushes the container image. Running
the workflow by hand first is a dry run that publishes nothing.
