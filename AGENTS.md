# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

## What this project is

Shotgun is a reverse proxy generator that translates between two REST API shapes. Given two OpenAPI specs (source = the API shape to expose, target = the upstream API to call), it diffs them, auto-maps matching endpoints/fields, and runs an axum-based proxy that translates requests and responses on the fly. Supports OpenAPI 3.0/3.1 and Swagger 2.0.

## Build and test

```sh
cargo build
cargo test
cargo test <test_name>        # run a single test
cargo build --release         # release build
```

No linter or formatter is configured beyond `cargo check`.

## Architecture

The pipeline has four stages, each in its own module under `src/`:

1. **`spec/`** — Parses OpenAPI 3.x and Swagger 2.0 into a version-agnostic `ApiSpec` model (`spec/model.rs`). The rest of the codebase never touches raw spec JSON.

2. **`diff/`** — Compares two `ApiSpec`s to produce a `MappingFile`. Endpoint matching uses normalized paths (ignoring `{param}` names) + HTTP method, then falls back to `operationId`. Field matching is by exact name with type compatibility checks.

3. **`mapping/`** — Serialization (`writer.rs`), deserialization (`reader.rs`), and merge logic (`merge.rs`) for the `mappings.toml` file. Types live in `types.rs`. The `MappingFile` struct is the central data structure used by both the diff engine and the proxy. The merge logic (`sync` command) preserves entries marked `edited = true`.

4. **`proxy/`** — The runtime reverse proxy.
   - `server.rs` — axum server setup
   - `handler.rs` — request routing via `match_endpoint()`, path/query param translation, upstream forwarding, response rewriting
   - `transform.rs` — applies renames, defaults, drops, and nested schema mappings to JSON request/response bodies
   - `pagination.rs` — Link header rewriting and query param translation for pagination

**CLI** (`cli.rs` / `main.rs`): Four subcommands — `init`, `serve`, `sync`, `validate`. Wired through `config.rs` which orchestrates the spec→diff→write pipeline.

## Key types

- `ApiSpec` (`spec/model.rs`) — normalized API representation
- `MappingFile` (`mapping/types.rs`) — the full mapping config (meta, settings, endpoints, schemas)
- `EndpointMapping` — maps one source route to one target route with field-level transforms
- `SchemaMapping` — reusable field map for types appearing in multiple endpoints (e.g. `User`)
- `FieldMapping` — renames, defaults, drops, type_conflicts, nested references

## Tests

Integration tests are in `tests/`:
- `endpoint_matching.rs` — `match_endpoint()` path matching
- `schema_matching.rs` — diff engine's schema/field matching
- `round_trip.rs` — serialize→deserialize round-trip for mapping files
- `transform.rs` — request/response body transformation

Test fixtures (OpenAPI specs) are in `tests/fixtures/`.

## The mapping file format

The `mappings.toml` format is central to the project. Key concepts:
- `[[endpoints]]` (double brackets) = list entries; `[endpoints.response.renames]` (single brackets) = sub-section of the most recent endpoint
- `edited = true` protects entries from being overwritten by `sync`
- Renames are never auto-generated — only humans add them
- Defaults synthesize source-only fields with zero values
- Nested objects delegate to named `[[schemas]]` entries
