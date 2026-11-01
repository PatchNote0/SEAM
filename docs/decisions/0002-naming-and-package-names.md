# 0002. Naming and package names

Status: accepted, 2026-11-01

## Context

The registries were checked on 2026-11-01. `seam`, `seam-core`, `seam-cli` and
`seam-engine` are taken on crates.io; `seam` and `seam-cli` are taken on npm by
Seam (seam.co), whose CLI also installs as `seam` through its Homebrew tap;
`seam` is taken on PyPI. Trademark availability was not assessed.

## Decision

- The product keeps the working name SEAM.
- Workspace crates use the `seam-` prefix and set `publish = false`, so an
  accidental `cargo publish` cannot claim or collide with someone else's name.
- The binary is called `seam`, as specified.

## Alternatives considered

- Renaming the product now. Deferred: the name is the owner's decision, and
  nothing is published yet.
- Publishable crate names such as `seam-contracts` or `seamctl`, both free when
  checked. Kept as candidates.

## Consequences

- On a machine that also has Seam's CLI, the two `seam` binaries collide; the
  one earlier on `PATH` wins. This must be resolved before any public package
  release, by renaming the binary or the product.
