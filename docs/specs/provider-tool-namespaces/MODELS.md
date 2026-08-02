---
title: "Provider Tool Namespace Wire Models"
created: 2026-08-02
updated: 2026-08-02
doc_type: "wire-models"
status: "proposed"
owner: "soma"
---

# Provider Tool Namespace Wire Models

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

`repositories` means `soma nexus repositories`, not `soma repositories`.

## CLI Parse Model

```text
argv: ["nexus", "repos", "--repo", "soma"]

ProviderCliInvocation {
  id: ProviderToolId { provider: "nexus", tool: "repos" },
  params: { "repo": "soma" },
  compatibility: Canonical
}
```

Provider-level and tool-level help are discovery operations and do not execute
provider code beyond the catalog lifecycle already required by live CLI
inspection.

## MCP Request

```json
{
  "provider": "nexus",
  "tool": "repos",
  "repo": "soma"
}
```

The generated MCP input schema uses conditional branches keyed by both
`provider` and `action`. Parameter names shared by different tools are merged
only as schema properties; the matching conditional branch controls validity.

## MCP Success

```json
{
  "_soma_provider": "nexus",
  "_soma_action": "repos",
  "output": {
    "items": []
  },
  "request_id": "req_01",
  "progress": [],
  "warnings": []
}
```

## REST Request and Success

```http
POST /v1/providers/nexus/tools/repos HTTP/1.1
content-type: application/json

{"repo":"soma"}
```

```json
{
  "provider": "nexus",
  "action": "repos",
  "output": {
    "items": []
  },
  "request_id": "req_01",
  "progress": [],
  "warnings": []
}
```

## Provider Discovery

`GET /v1/providers` exposes canonical routes and local aliases:

```json
{
  "providers": [
    {
      "name": "nexus",
      "tools": [
        {
          "name": "repos",
          "identity": { "provider": "nexus", "tool": "repos" },
          "display_name": "nexus.repos",
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
            }
          }
        }
      ]
    }
  ]
}
```

## Structured Error

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

## Legacy Success Warning

```json
{
  "code": "legacy_flat_action",
  "message": "Flat action `nexus_repos` is deprecated; use `nexus.repos`.",
  "canonical_provider": "nexus",
  "canonical_tool": "repos"
}
```

Warnings are returned in envelopes where supported and emitted through
structured diagnostics/logging otherwise. They never include credentials or
raw input values.

## Ambiguous Legacy Failure

If both `nexus.status` and `weather.status` exist, a provider-less `status`
compatibility call fails:

```json
{
  "code": "ambiguous_legacy_action",
  "legacy_action": "status",
  "candidates": [
    { "provider": "nexus", "tool": "status" },
    { "provider": "weather", "tool": "status" }
  ]
}
```

Candidates are sorted for deterministic diagnostics.
