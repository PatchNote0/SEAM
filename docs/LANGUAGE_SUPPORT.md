# Language support

A language or format is listed as supported only when its adapter exists, its
capabilities are declared, its tests pass and the linker uses what it reads.
`seam languages` prints every declared capability, including what is not
handled.

| Language or format | Status | What is read |
|---|---|---|
| TypeScript | supported, 0.1.0 | `fetch` and `axios` requests, wrapper functions, constants, relative imports, environment variables with fallbacks, interfaces and type aliases, `it`/`test` blocks |
| JavaScript | supported, 0.1.0 | the same, through the TSX grammar, without types |
| Python (FastAPI) | supported, 0.1.0 | routes, routers, prefixes, `include_router`, Pydantic models, aliases, request bodies, `TestClient` requests |
| OpenAPI 3.0 and 3.1 | supported, 0.1.0 | operations, request and response schemas, local `$ref`, `allOf`, `oneOf`, `anyOf`, nullability |
| Docker Compose | supported, 0.1.0 | services, build contexts, environment |
| Manifests | supported, 0.1.0 | component names from `package.json`, `pyproject.toml`, `Cargo.toml` and `go.mod` |
| Rust (Axum, Serde) | planned for Phase 4 | |
| SQL | planned for Phase 4 | dialect-aware parsing of migrations and queries |
| Python HTTP clients (httpx, requests) | planned | |
| GraphQL, Go, Java and others | later | see `IMPLEMENTATION_PLAN.md` |

## Known limits

- TypeScript path aliases from `tsconfig.json`, package imports, re-exports,
  class methods, `axios.create` instances and inferred types are not
  followed. A call that depends on them is recorded with an assumption or as
  `UNKNOWN`.
- A wrapper is recognised when one of its parameters supplies the start of
  the request path. A function that only fills a segment, such as
  `search(collection)` building `/${collection}/search`, is reported once with
  an `UNKNOWN` path rather than once per caller.
- FastAPI dependencies, `APIRoute` subclasses, mounted sub-applications and
  routes added with `add_api_route` are not read.
- Pydantic `model_config` alias generators are not applied.
- Compose `env_file` and `${VAR}` interpolation are not read.
