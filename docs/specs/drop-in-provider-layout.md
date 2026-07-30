---
title: "Drop-In Provider Layout"
created: 2026-07-15
updated: 2026-07-30
---

# Drop-In Provider Layout

## Status

Implemented. See `docs/contracts/drop-in-provider-layout.md` for the
normative implementation checklist this spec's intent was promoted into.

## Goal

Make provider authoring feel like adding files to a well-known folder, not
like assembling a platform manifest by hand. The runtime infers the common
case from path and extension:

```text
providers/
  tools/
  prompts/
  resources/
```

The directory name selects the MCP primitive:

| Directory | Primitive | Happy path |
|---|---|---|
| `providers/tools/` | MCP tools plus optional CLI/REST overlays | Drop `.json`, `.ts`, `.wasm`, or `.py` files |
| `providers/prompts/` | MCP prompts | Drop `.md` files |
| `providers/resources/` | MCP resources and resource templates | Drop static files or dynamic `.ts` reader files |

The core rule is that a file path is useful by itself. Metadata may refine
the surface, but it is never required for the happy path.

## Non-Goals

- Do not require JSON manifests for static prompts or static resources.
- Do not expose prompts or resources through REST or CLI by default (matches
  this project's documented MCP-only exception for prompts/resources — see
  `CLAUDE.md`'s CLI ↔ MCP action parity table).
- Do not execute dynamic resource handlers during non-executing inspection
  (`soma providers list|lint|status`).
- Do not let resource files escape the configured provider directory through
  symlinks, `..`, or path tricks.
- Do not make root-level provider files ambiguous once the structured layout
  is available — both continue to work side by side.

## Directory Semantics

### Tools

`providers/tools/` owns action-like provider files:

```text
providers/tools/weather.ts
providers/tools/github.openapi.json
providers/tools/analyzer.wasm
```

These load into the existing provider tool registry exactly as their
root-level equivalents would. They may expose MCP tools, CLI commands, REST
routes, OpenAPI, Palette commands, and generated docs according to their
provider metadata.

### Prompts

`providers/prompts/` owns prompt templates:

```text
providers/prompts/code-review.md
providers/prompts/release-notes.md
```

Each Markdown file becomes one MCP prompt. The filename defines the prompt
name, the first `# Heading` provides the description when present, and the
full Markdown file becomes the returned prompt message.

Prompt files are not tools. They are not callable actions and do not get CLI
or REST exposure.

### Resources

`providers/resources/` owns URI-addressable context:

```text
providers/resources/runbook.md
providers/resources/api/schema.json
providers/resources/service/[name].ts
providers/resources/repo/file/[...path].ts
```

Static files become resources directly. `.ts` files become resource
readers. The path under `providers/resources/` defines the URI.

## Resource URI Mapping

Static resource:

```text
providers/resources/runbook.md
```

maps to:

```text
soma://resources/runbook
```

Nested static resource:

```text
providers/resources/api/schema.json
```

maps to:

```text
soma://resources/api/schema
```

Dynamic resource:

```text
providers/resources/status.ts
```

maps to:

```text
soma://resources/status
```

Parameterized dynamic resource:

```text
providers/resources/service/[name].ts
```

maps to:

```text
soma://resources/service/{name}
```

Catch-all dynamic resource:

```text
providers/resources/repo/file/[...path].ts
```

maps to:

```text
soma://resources/repo/file/{path}
```

URI matching prefers exact static resources, then exact (zero-parameter)
dynamic resources, then parameterized resources, then catch-all resources.
Ambiguous templates at the same tier are rejected when the provider
directory loads.

## Static Resources

The runtime infers:

- `name` from the path stem;
- `uri` from the path relative to `providers/resources/`;
- `mime_type` from extension;
- `description` from the first Markdown heading for Markdown files, or a
  generated description for other file types.

Text MIME types return `ResourceContents::text`. Binary MIME types return
`ResourceContents::blob`.

## Dynamic Resources

Dynamic resources are `.ts` files. The minimal dynamic resource is:

```ts
export async function read(input) {
  return { text: "current status" };
}
```

Parameterized resources receive params extracted from the URI:

```ts
export async function read(input) {
  return { text: `status for ${input.params.name}` };
}
```

Dynamic readers return one of:

```ts
return { text: "plain text", mimeType: "text/plain" };
return { json: { ok: true } };
return { blob: base64Bytes, mimeType: "image/png" };
```

No exported manifest is required — path-derived defaults are sufficient.

WASM resources are not implemented. A resource WASM ABI can be added later,
following the same path-to-URI convention, once real usage of the
TypeScript reader contract justifies it.

## Validation

Non-executing inspection (`soma providers list|lint|status`) reports tools,
prompts, and resources separately, validates path-derived names and URIs,
rejects duplicate resource URIs and ambiguous URI templates, rejects invalid
dynamic parameter names, and rejects symlink escapes — all without executing
dynamic readers, instantiating WASM, calling MCP upstreams, or fetching
OpenAPI URLs.

## MCP Runtime Behavior

The MCP server refreshes provider files on `tools/list`, `prompts/list`,
`prompts/get`, `resources/list`, and `resources/read`. `resources/list`
returns static resources; `resources/templates/list` returns dynamic
resource templates. `resources/read` resolves the URI against the active
provider snapshot and returns the matching resource contents.

If a directory refresh fails (an invalid file, a collision, a symlink
escape), the last valid snapshot stays active rather than the request
failing — see "Refresh Failure Handling" in the contract doc.

## Migration Path

1. Root-level `.json`, `.ts`, `.wasm`, `.py`, and `.md` support remains for
   compatibility.
2. Structured directory discovery for `tools/`, `prompts/`, and `resources/`
   is implemented and available today.
3. Prefer the structured layout in new examples and docs (done —
   `examples/providers/resources/` demonstrates it).
4. Optionally warn when root-level files are used after the structured
   layout is broadly adopted — not implemented; revisit if root-level usage
   becomes a support burden.
