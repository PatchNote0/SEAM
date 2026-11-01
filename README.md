# SEAM

Evidence-first cross-language contract intelligence.

> Understand what connects your software, prove why it connects, and discover
> what changes when those connections break.

SEAM finds the boundaries between independently built parts of a system, such
as a TypeScript client calling a Python API that reads a Postgres table, and
records each one as a **Seam** with its producer, consumer, contract, field
mappings, evidence and an honest classification of how well it is proven.

**Status: early development.** The domain model and its rules exist and are
tested; the language adapters, storage and the `seam` command are being built.
See [docs/PROJECT_STATUS.md](docs/PROJECT_STATUS.md) for exactly what works.

## Building

```sh
cargo test --workspace
```

## Documents

- [Product differentiation](docs/PRODUCT_DIFFERENTIATION.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Data model](docs/DATA_MODEL.md)
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Security and privacy](docs/SECURITY.md)
- [Decisions](docs/decisions/)
- [Contributing](CONTRIBUTING.md)
