---
title: "Drop-in provider layout contract"
doc_type: "contract"
status: "implemented"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "product"
source_of_truth: true
upstream_refs:
  - "docs/specs/drop-in-provider-layout.md"
  - "docs/contracts/provider-manifest.schema.json"
  - "crates/soma/application/src/providers/filesystem.rs"
  - "crates/soma/application/src/providers/resource_files.rs"
  - "crates/soma/application/src/providers/resource_uri.rs"
  - "crates/soma/application/src/provider_registry.rs"
  - "crates/soma/mcp/src/rmcp_server.rs"
last_reviewed: "2026-07-15"
---

# Drop-In Provider Layout Contract

This contract pins the implemented behavior for the structured provider directory:

```text
providers/
  tools/
  prompts/
  resources/
```

The spec explains the design intent. This document is the implementation
checklist and reflects `main` as built, not the original draft verbatim.

## Provider Root

The provider root is selected by `SOMA_PROVIDER_DIR`, an explicit `--dir
<path>` argument, or the default `./providers`.

The selected provider root is the trust boundary for path resolution. No
resource file, symlink, or nested path may resolve outside that root.
Enforcement lives in `filesystem::walk_resources_dir`: every entry is
canonicalized and checked with `starts_with(canonical_root)` before being
trusted; a symlink that escapes the root fails the whole directory scan
rather than being silently skipped (the live server keeps serving its last
valid snapshot instead of crashing — see "Refresh Failure Handling" below).

## Directory Ownership

| Directory | Implemented behavior |
|---|---|
| `tools/` | Loads `.json`, `.ts`, `.wasm`, and `.py` provider tool files. Flat (non-recursive), same file-type rules as root. |
| `prompts/` | Loads `.md` prompt files. Flat (non-recursive). `README.md` is excluded (case-insensitive). |
| `resources/` | Loads static resource files and `.ts` dynamic resource reader files, recursively. |

Root-level file loading remains supported for compatibility; new examples
and docs prefer the structured layout.

## Tool Files

Unchanged from the existing provider manifest contract
(`docs/contracts/provider-manifest.schema.json`,
`docs/PROVIDERS.md`) — `tools/` is just an additional directory
`FileProviderSource` scans with the same extension rules as root.

## Prompt Files

Each Markdown file under `prompts/` (or root) becomes one MCP prompt. See
"Markdown Prompts" in `docs/PROVIDERS.md` for the name/description derivation
rules — they are identical for root-level and `prompts/`-directory files.

## Static Resource Files

Every non-`.ts` file under `resources/` becomes an MCP resource.

The resource URI is derived from the path relative to `resources/`:

```text
providers/resources/api/schema.json
```

maps to:

```text
soma://resources/api/schema
```

The final file extension is removed. Each path segment is normalized with
the same separator rules as prompt names (lowercase, punctuation collapsed
to a single `-`) — see `resource_uri::slugify`.

| Field | Rule |
|---|---|
| `uri_template` | `soma://resources/{normalized/path/without/extension}` |
| `name` | joined normalized path segments |
| `description` | first Markdown heading (for `.md` files) or `Resource `{name}`` |
| `mime_type` | inferred from file extension (`resource_files::mime_type_for_extension`), `application/octet-stream` if unknown |

Text MIME types (`text/*`, `application/json`, `application/yaml`,
`application/toml`, `application/xml`) return `ResourceContents::text`.
Everything else returns `ResourceContents::blob` (base64).

Static resource files larger than `resource_files::MAX_STATIC_RESOURCE_BYTES`
(10 MiB) are rejected at discovery time.

## Dynamic Resource Reader Files

`.ts` files under `resources/` become MCP resource templates, dispatched
through the same sandboxed Node sidecar (`providers::sidecar`) that
`ai-sdk`-kind tool providers already use — `env_clear()`'d, timeout- and
output-size-bounded, no explicit env vars passed by default.

Path parameters use bracket segments, matching the file path exactly:

| File path | URI template |
|---|---|
| `status.ts` | `soma://resources/status` |
| `service/[name].ts` | `soma://resources/service/{name}` |
| `repo/file/[...path].ts` | `soma://resources/repo/file/{path}` |

Parameter names must match `^[A-Za-z_][A-Za-z0-9_]*$`. At most one catch-all
(`[...name]`) segment may appear, and it must be the final segment
(`resource_uri::parse_resource_path` rejects anything else at discovery
time).

TypeScript dynamic resource files must export:

```ts
export async function read(input) { ... }
```

`input` is `{ uri: string, params: Record<string, string>, query:
Record<string, string> }` (`query` is parsed from the request URI's `?...`
suffix via `url::form_urlencoded`, empty object if absent).

The return value must be one of:

```ts
{ text: string, mimeType?: string }
{ json: unknown }
{ blob: string, mimeType: string }
```

`json` results are serialized as `application/json` text. `blob` requires
`mimeType` — there is no default for binary content. Any other shape (or a
non-object return value) is rejected as `resource_reader_invalid_shape`.

**Not implemented**: a WASM dynamic resource reader ABI. The upstream spec
explicitly deferred this until the TypeScript contract was proven; it still
is. `.ts` is the only reader extension recognized today.

## URI Matching

`ProviderRegistry::match_resource` resolves a request URI in this order:

1. exact static resource (`catalog.resources[].uri_template`, `O(1)` hash
   lookup);
2. dynamic templates, tried in an order that puts literal-only (zero-param)
   templates before parameterized ones before catch-all ones — so a
   `status.ts` reader and a sibling `[name].ts` reader can coexist, with the
   more specific one winning for an exact match.

Two dynamic templates are rejected as ambiguous at snapshot-build time
(`duplicate_resource_uri`/`ambiguous_resource_template`) when they have
identical segment *shape* (same literal positions and values, same
parameter/catch-all positions) regardless of parameter name — e.g.
`service/[name].ts` and `service/[id].ts` conflict; `service/[name].ts` and
`team/[id].ts` do not.

## Refresh Failure Handling

If a directory refresh fails for any reason — an unreadable directory, a
newly invalid or colliding provider file, a symlink escape — the server logs
a warning and keeps serving the last valid snapshot rather than failing
`tools/list`, `prompts/list`, `prompts/get`, `resources/list`, or
`resources/read` for every other, unrelated, already-loaded provider. See
`ProviderRegistry::refresh_file_providers`.

## Scope Enforcement

`resource.scope` (like `prompt.scope` and `tool.scope`) is enforced via
`scopes_satisfy` under `ProviderAuthMode::Mounted` only, at the point of use
(`read_resource`), not at listing time — mirroring how `tool.scope` is
enforced at `call_tool`, not `list_tools`.

## Non-Executing Inspection (`soma providers list|lint|status`)

Resource files are reported by `FileProviderSource::inspect()` alongside
tools and prompts, using the same `ProviderFileInspection` shape
(`provider_id`, `provider_kind`, `error`). Static resource files are fully
inspected without execution. Dynamic `.ts` reader files are also inspected
without execution — no sidecar is spawned; the file is only parsed for its
path-derived template shape, matching how `soma providers lint` never
executes `ai-sdk` TS providers either.

## Verification

```bash
cargo test -p soma-application providers::resource_uri
cargo test -p soma-application providers::resource_files
cargo test -p soma-application providers::filesystem
cargo test -p soma --test provider_registry
cargo test -p soma --test drop_provider_probe
```
