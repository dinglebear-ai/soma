---
title: "Provider Tool Namespace Wire Models"
created: 2026-08-02
updated: 2026-08-02
doc_type: "wire-models"
status: "proposed"
owner: "soma"
---

# Provider Tool Namespace Wire Models

These examples distinguish canonical application fields (`provider`, `tool`)
from MCP's transport spelling (`provider`, `action`).

## Manifest v2

```json
{
  "schema_version": 2,
  "provider": {
    "name": "nexus",
    "kind": "python",
    "title": "Nexus"
  },
  "tools": [
    {
      "name": "repos",
      "description": "List repositories and checkout state.",
      "input_schema": {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "repo": { "type": ["string", "null"] }
        }
      },
      "mcp": { "enabled": true },
      "rest": { "enabled": true },
      "cli": { "enabled": true, "aliases": ["repositories"] }
    }
  ]
}
```

Python selects the manifest semantics separately from other SDK/protocol
versions:

```python
PROVIDER = provider(manifest_version=2, name="nexus", kind="python")
```

`repositories` means `soma nexus repositories`, not `soma repositories`.

## Canonical Application Request

```json
{
  "provider": "nexus",
  "tool": "repos",
  "params": { "repo": "soma" }
}
```

## CLI Parse Model

```text
argv: ["nexus", "repos", "--repo", "soma"]

ProviderCliInvocation {
  id: ProviderToolId { provider: "nexus", tool: "repos" },
  params: { "repo": "soma" }
}
```

Provider-level and tool-level help are discovery operations. They use one
immutable catalog snapshot but do not claim a later command sees the same
generation.

## Confirmation Preflight

```json
{
  "provider": "nexus",
  "tool": "keys",
  "snapshot_id": "sha256:abc123",
  "destructive": false,
  "requires_admin": true,
  "required_scope": "soma:read"
}
```

If policy changes after an interactive prompt, final dispatch rejects the old
proof:

```json
{
  "kind": "provider_tool_error",
  "schema_version": 1,
  "code": "stale_provider_confirmation",
  "provider": "nexus",
  "tool": "keys",
  "message": "Provider tool policy changed after confirmation preflight.",
  "retryable": true,
  "remediation": "Resolve the tool again and repeat confirmation."
}
```

## MCP Request

```json
{
  "provider": "nexus",
  "action": "repos",
  "repo": "soma"
}
```

The generated MCP input schema contains one complete Draft 2020-12 object
branch per provider/action pair. Each branch requires both discriminators with
`const` and contains only that tool's parameter schema plus shared paging
fields. Parameter definitions are not globally merged.

## MCP Success

The following object is `CallToolResult.structuredContent`:

```json
{
  "_soma_provider": "nexus",
  "_soma_action": "repos",
  "output": {
    "items": []
  },
  "request_id": "req_01",
  "progress": []
}
```

`CallToolResult.content` also contains a text block whose parsed JSON
normalizes to the same object for older clients.

## MCP Structured Error

Unknown provider/action inside the MCP tool `soma` is a tool-result error:

```json
{
  "isError": true,
  "structuredContent": {
    "kind": "provider_tool_error",
    "schema_version": 1,
    "code": "unknown_provider_tool",
    "provider": "nexus",
    "action": "missing",
    "message": "Provider `nexus` has no tool `missing`.",
    "retryable": false,
    "remediation": "Inspect the current provider catalog."
  }
}
```

## REST Request and Canonical Success

```http
POST /v1/providers/nexus/tools/repos HTTP/1.1
content-type: application/json

{"repo":"soma"}
```

```json
{
  "provider": "nexus",
  "tool": "repos",
  "output": {
    "items": []
  },
  "request_id": "req_01",
  "progress": []
}
```

## Live OpenAPI Operation

Runtime uses a generic capture route, but live OpenAPI enumerates the concrete
loaded operation:

```json
{
  "/v1/providers/nexus/tools/repos": {
    "post": {
      "operationId": "provider_5_nexus_tool_5_repos",
      "x-soma-provider": "nexus",
      "x-soma-tool": "repos",
      "requestBody": {
        "content": {
          "application/json": {
            "schema": { "type": "object" }
          }
        }
      },
      "responses": {
        "200": { "description": "Canonical provider tool result" }
      }
    }
  }
}
```

The length-prefixed operation ID is illustrative; implementation may use
another deterministic injective encoding or stable digest suffix.

## Provider Discovery

`GET /v1/providers` exposes identity, schemas, concrete routes, aliases, and
generation:

```json
{
  "generation_id": 42,
  "fingerprint": "sha256:abc123",
  "canonical_rest_template": "/v1/providers/{provider}/tools/{tool}",
  "providers": [
    {
      "name": "nexus",
      "manifest_version": 2,
      "tools": [
        {
          "name": "repos",
          "identity": { "provider": "nexus", "tool": "repos" },
          "display_name": "nexus.repos",
          "input_schema": { "type": "object" },
          "output_schema": { "type": "object" },
          "surfaces": {
            "cli": {
              "command": ["nexus", "repos"],
              "aliases": [["nexus", "repositories"]]
            },
            "mcp": {
              "tool": "soma",
              "provider": "nexus",
              "action": "repos"
            },
            "rest": {
              "method": "POST",
              "path": "/v1/providers/nexus/tools/repos"
            },
            "palette": {
              "provider": "nexus",
              "tool": "repos",
              "display_id": "nexus.repos"
            }
          }
        }
      ]
    }
  ]
}
```

## Palette Model

```json
{
  "provider": "nexus",
  "tool": "repos",
  "display_id": "nexus.repos",
  "title": "Repositories",
  "description": "List repositories and checkout state.",
  "input_schema": { "type": "object" }
}
```

Palette schema lookup and execution send `provider` and `tool`; they do not
split `display_id`.

## Non-Executing Python Inspection

```json
{
  "file_name": "nexus.py",
  "status": "runtime-validation-required",
  "provisional_provider": "nexus",
  "declared_provider": null,
  "tools": [],
  "code": "python_runtime_validation_required",
  "message": "Python catalog discovery requires contained execution; run `soma providers validate`."
}
```

The provisional file stem is diagnostic, not confirmed API identity.

## Canonical Structured Error

```json
{
  "kind": "provider_tool_error",
  "schema_version": 1,
  "code": "unknown_provider_tool",
  "provider": "nexus",
  "tool": "missing",
  "message": "Provider `nexus` has no tool `missing`.",
  "retryable": false,
  "remediation": "Run `soma nexus --help` or inspect GET /v1/providers."
}
```

## Refresh Event

```json
{
  "generation_id": 43,
  "fingerprint": "sha256:def456",
  "added": [{ "provider": "nexus", "tool": "shares" }],
  "removed": [],
  "surface_changes": [{ "provider": "nexus", "tool": "repos" }],
  "schema_changed": true
}
```

A successful event with `schema_changed: true` drives MCP
`notifications/tools/list_changed`. A rejected candidate emits neither this
event nor the MCP notification and retains the prior generation.
