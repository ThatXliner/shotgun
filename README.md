# Shotgun

Shotgun is a reverse proxy generator for OpenAPI specs. Point it at two API
descriptions — the interface you want to *expose* (source) and the upstream
service you're actually calling (target) — and it diffs them, infers field
and endpoint mappings, and runs a proxy that translates requests and
responses between the two shapes.

The idea: two APIs describing similar domains (two Git forges, two payment
processors, two CMS platforms) usually share 60-80% of their structure.
Shotgun auto-maps the obvious parts and surfaces the rest for you to fill in.

## Install / build

```sh
cargo build --release
./target/release/shotgun --help
```

## Usage

```sh
# 1. Diff the two specs and generate a mapping file.
shotgun init --source github-api.json --target forgejo-api.json --output mappings.toml

# 2. Review mappings.toml, fill in anything left with target = "".

# 3. Run the proxy.
shotgun serve --mappings mappings.toml --target-url https://forgejo.example.com
```

When either spec changes, re-diff without losing your hand-edits:

```sh
shotgun sync --source github-api-v2.json --target forgejo-api-v2.json --mappings mappings.toml
```

Check a mapping file for problems and coverage stats at any time:

```sh
shotgun validate --mappings mappings.toml
```

## How it works

### `shotgun init`

Shotgun's matching is **deterministic** — an endpoint or field either maps
unambiguously or it doesn't. There's no scoring, no fuzzy path similarity,
no "73% confident" middle ground, and no guessed renames. A wrong guess
that looks confident is worse than an honest gap, so Shotgun never guesses.

1. Parses both specs (OpenAPI 3.0/3.1 and Swagger 2.0 are both supported,
   normalized into one internal model).
2. Matches source endpoints to target endpoints:
   - **Path match**: normalize both paths (strip the base path, replace
     `{param}` segments with a positional placeholder) and look for an
     exact method + normalized-path match. `GET /repos/{owner}/{repo}` and
     `GET /repos/{user}/{project}` match; the param *names* don't matter.
   - **`operationId` match**: if no path match, fall back to an exact
     (case-insensitive) `operationId` match — this catches the same logical
     operation living at a different path.
   - **Method mismatch**: if the normalized path matches but the HTTP
     method doesn't (`PUT` vs `POST` on the same path), it's deliberately
     left unmapped with a note, rather than assumed correct.
   - Anything else is unmapped, listed with `target = ""` for a human to
     fill in.
3. For every matched pair, diffs the request/response field shapes:
   - same name + compatible type → nothing to do, already auto-mapped at
     runtime because the key is identical on both sides
   - same name + incompatible type → flagged in `type_conflicts`, left
     untouched (no silent coercion)
   - field in source only → `defaults`
   - field in target only → `drops`
   - **No rename inference.** If `full_name` (target) and `name` (source)
     mean the same thing, Shotgun won't guess — it lists `name` as a
     default and `full_name` as a drop, and a human adds the rename.
   - Nested named schemas (e.g. a `User` embedded in a `Repository`) are
     recursively diffed once and registered as reusable `[[schemas]]`
     entries.
4. Writes `mappings.toml` and prints a coverage summary.

### `shotgun serve`

Starts an axum server that, for every request:

1. Matches the request against a `[[endpoints]]` entry (405/501 if unmapped,
   per `settings.unmapped_endpoint_behavior`).
2. Rewrites the path (applying `target_base_path` and renamed path params).
3. Renames query params and headers per the mapping.
4. Transforms the JSON request body (source field names → target field
   names) and forwards it upstream.
5. Transforms the JSON response body back (target → source), applying
   renames, defaults, drops, and nested schema maps recursively — arrays are
   handled element-by-element.
6. Rewrites pagination (`Link` headers, page-size param names) if configured.

The proxy operates on `serde_json::Value` trees, not typed structs — it has
no idea what a "repository" or "issue" is, only that "this JSON key maps to
that JSON key." That's what makes it work for any API pair.

### `shotgun sync`

Re-runs the diff against updated specs and merges the result into your
existing `mappings.toml`: entries you've hand-edited (`edited = true`) are
kept as-is, auto-generated entries are refreshed, new endpoints are added,
and endpoints that disappeared from the source spec are flagged rather than
silently deleted.

## The mapping file

`mappings.toml` is the actual product here — everything else is plumbing.
It's meant to be read, diffed in version control, and hand-edited.

```toml
[[endpoints]]
source = "GET /repos/{owner}/{repo}"
target = "GET /repos/{owner}/{repo}"
# edited = true   # set by hand once you touch an auto-generated entry;
                   # `shotgun sync` won't overwrite edited entries.

  [endpoints.response.renames]
  # source_field = "target_field"   # you add these; Shotgun never guesses

  [endpoints.response.defaults]
  node_id = ""   # GitHub-specific; Forgejo has no equivalent

  [endpoints.response.drops]
  # target-only fields to hide from source-API clients

  [endpoints.response.type_conflicts]
  # field = "source is String, target is Object"  # same name, incompatible
  # shape -- needs a human decision, never auto-resolved

  [[endpoints.response.nested]]
  path = "owner"
  schema_map = "user"   # reuses a [[schemas]] entry
```

An endpoint is either mapped (`target` is non-empty) or it isn't — there's
no confidence spectrum. Every unmapped entry carries a `note` explaining
*why* auto-diff didn't map it: no path/operationId match at all, or a path
match with a conflicting HTTP method. `edited = true` marks an entry a human
has touched, which `shotgun sync` treats as authoritative and leaves alone
on the next re-diff.

Renames read as "the source API's field X corresponds to the target API's
field Y." The proxy applies them target→source for responses and
source→target for requests. Shotgun never writes a rename itself — it only
auto-maps fields with identical names — so every rename in the file is
either something a human added, or absent (meaning: go look at the
`defaults`/`drops` entries for that field name on each side and decide).

`[[schemas]]` entries are reusable field maps for named object types (e.g.
`User`, `Repository`) that show up in many endpoints — defined once,
referenced via `schema_map` wherever that shape appears nested in a
response.

`[settings]` controls proxy-wide behavior: how strict to be about unmapped
endpoints/fields, the target API's base path, and pagination handling
(param-name translation, `Link` header rewriting).

## Comparison to existing tools

Nothing found does exactly this — auto-diff two independently-authored
OpenAPI specs and generate a bidirectional, editable field-mapping proxy
between them — but several tools solve adjacent pieces of the problem:

| Tool | What it does | Why it's not this |
|---|---|---|
| [Kong `request-transformer`/`response-transformer`](https://developer.konghq.com/plugins/request-transformer/) | Rename/add/remove headers, query params, and body fields on the way through a gateway | Every rule is hand-written; there's no OpenAPI-aware diffing step that proposes mappings from two specs |
| [AWS API Gateway mapping templates](https://docs.aws.amazon.com/apigateway/latest/developerguide/models-mappings.html) | Transform request/response payloads per-integration using VTL scripts | Same as above — a scripting target, not a diff-and-generate workflow; one template per endpoint, written by hand |
| [Apigee](https://docs.apigee.com/api-platform/tutorials/create-api-proxy-openapi-spec) | Scaffolds an API proxy *from* a single OpenAPI spec | Generates a passthrough proxy for one spec, not a translator between two different specs |
| [grpc-gateway](https://github.com/grpc-ecosystem/grpc-gateway) / Envoy gRPC-JSON transcoder | Translates RESTful JSON to gRPC using one `.proto` as the single source of truth | Protocol translation from a single schema, not shape translation between two independently-evolved REST APIs |
| [GraphQL Mesh](https://github.com/ardatan/graphql-mesh) / [WunderGraph](https://docs.wundergraph.com/docs/supported-data-sources/rest-openapi) | Unify multiple REST/GraphQL/gRPC sources behind one GraphQL (or generated JSON) API | The unifying layer is GraphQL, not a REST API shaped like an existing spec; no direct REST-in/REST-out passthrough with per-field mapping review |
| [oasdiff](https://github.com/oasdiff/oasdiff) / [openapi-diff](https://github.com/OpenAPITools/openapi-diff) | Diff two versions of *the same* API and flag breaking changes | Compares one API against itself over time for CI gating — doesn't match endpoints/fields across two *different* APIs or generate a runnable proxy |
| [mitmproxy2swagger](https://github.com/alufers/mitmproxy2swagger) | Reverse-engineer an OpenAPI spec from captured HTTP traffic | Produces a spec from traffic; doesn't map or proxy between two specs |

The closest analogues are gateway *transformation* plugins (Kong, AWS
mapping templates) — but they're scripting targets you write by hand for
one endpoint at a time. Shotgun's difference is upstream of that: it reads
both OpenAPI specs, deterministically figures out what already lines up,
and generates the starting point so you're editing a diff instead of
writing one from a blank file.

## Project layout

```
src/
  spec/    OpenAPI 2.0/3.x parsing into a normalized internal model
  diff/    endpoint + schema matching, mapping-file generation
  mapping/ mapping file types, TOML/YAML/JSON I/O, sync merge logic
  proxy/   axum server, generic request handler, JSON transform engine
tests/     diff engine + proxy transform tests, Petstore v2/v3 fixtures
examples/github-forgejo/  reference mapping for GitHub -> Forgejo
```

## Status

Milestones 1-5 from the design doc are implemented: spec parsing, the
auto-diff engine, the runtime proxy (including pagination/header handling),
and `sync`/`validate`. This is a working core, not a finished product.
Matching is intentionally conservative — deterministic path/operationId
matching for endpoints, exact-name matching for fields, no fuzzy guessing
anywhere — which means coverage numbers on `shotgun init` can look lower
than a fuzzier tool would report. That's by design: every auto-generated
entry is something you can verify by eye against the two spec files, and
the `defaults`/`drops` entries left behind for near-miss fields (like
`name` vs `full_name`) are exactly the todo list for turning them into
proper renames.
