# Language support

A language or format is listed as supported only when its adapter exists, its
capabilities are declared, and its tests pass. Today none are.

| Language or format | Status | Planned approach |
|---|---|---|
| TypeScript | planned for Phase 2 | tree-sitter; `fetch` calls, wrappers, interfaces, imports |
| JavaScript | planned for Phase 2 | the TypeScript adapter with the TSX grammar, without types |
| Python | planned for Phase 2 | tree-sitter; FastAPI routes and routers, Pydantic models |
| OpenAPI | planned for Phase 2 | spanned YAML and JSON reader; operations, schemas, local `$ref` |
| Docker Compose | planned for Phase 2 | services, build contexts, environment |
| Rust | planned for Phase 4 | tree-sitter; Axum routes, Serde models |
| SQL | planned for Phase 4 | dialect-aware parsing of migrations and queries |
| GraphQL, JSON, YAML, Go, Java and others | later | see `IMPLEMENTATION_PLAN.md` |
