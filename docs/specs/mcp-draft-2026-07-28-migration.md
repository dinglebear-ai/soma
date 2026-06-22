# MCP Draft Spec (2026-07-28) Compatibility

## Status

In progress. Tracked by beads epic `rmcp-template-b4q`.

The upcoming MCP draft revision is dated 2026-07-28. It is not final until then,
and the schema keeps changing. This document records the migration plan and which
work is safe to do now versus blocked upstream.

## Key finding: most of the migration is blocked on rmcp

The defining draft changes are protocol-level and live inside the `rmcp` crate,
version-gated by `ProtocolVersion`. As of 2026-06-21, rmcp draft support is
effectively absent:

- crates.io tops out at rmcp 1.7.0 (our pin), which has no draft awareness.
- `ProtocolVersion::V_2026_07_28` exists only on rmcp `main` / the unreleased
  1.8.0 (release PR modelcontextprotocol/rust-sdk#850 is still open).
- The only draft-gated behavior implemented upstream is SEP-2164 (the resource
  not-found error code change -32002 to -32602, PR #899).
- The defining draft features (stateless lifecycle, server/discover,
  subscriptions/listen, MRTR / InputRequiredResult, resultType, CacheableResult,
  capabilities.extensions) have no rmcp code, no merged PRs, and no tracking
  issues. rmcp LATEST is deliberately still 2025-11-25.

So full protocol compatibility cannot be built today. The chosen approach is to
land the non-protocol prep that is safe now and stage the protocol work behind an
rmcp upgrade.

## What the draft changes, and who owns each change

| Draft change | Owner | Status |
|---|---|---|
| Stateless lifecycle (drop initialize, per-request `_meta`) | rmcp | Blocked. We already run stateless-mode + json_response. |
| Remove `Mcp-Session-Id` | rmcp | Blocked (rmcp-internal). |
| `server/discover` RPC | rmcp (+ our handler hook) | Blocked. |
| `subscriptions/listen` | rmcp | Blocked. Low impact: we have no resource subscriptions. |
| `resultType` on all results | rmcp | Blocked. |
| MRTR replaces `elicitation/create` | rmcp + us | Blocked. Our two `peer.elicit()` calls are the only server-initiated requests. |
| Error renumbering -32020..-32099 | rmcp + us | Blocked. Our payloads use string `code`, insulated from numeric renumbering. |
| RFC 9207 `iss` in auth responses | us | Done (b4q.3). |
| `application_type` in DCR | us | Done (b4q.4). |
| `Mcp-Method` / `Mcp-Name` / `x-mcp-header` headers | rmcp + us | CORS allow-list done (b4q.2); emission is rmcp's (SEP-2243, PR #907, open). |
| Client ID Metadata Documents (CIMD) replacing DCR | us | Later / draft-coupled (b4q.9). |
| OTel `_meta` trace-context propagation | us | Not started. |

We use none of the deprecated features (Roots, Sampling, Logging, Tasks),
`tools/list` order is already deterministic, and our error payloads are insulated
from the numeric renumbering, so our real change surface is small.

## Done so far (safe deltas)

- RFC 9207 `iss` on OAuth authorization success and error responses (b4q.3).
- `application_type` accepted, validated, and echoed in dynamic client
  registration (b4q.4).
- MCP protocol headers allowed in CORS preflight (b4q.2).
- Conformance harness and baseline (b4q.1).
- This documentation (b4q.5).

## Blocked on rmcp upstream

- Upgrade rmcp to >= 1.8.0 once released (b4q.6).
- Migrate elicitation to MRTR / InputRequiredResult (b4q.7).
- Adopt stateless lifecycle, server/discover, subscriptions/listen, resultType,
  capabilities.extensions when rmcp implements them (b4q.8).

## Draft schema reference

The draft TypeScript schema is an upstream document, so it lives under the
gitignored `docs/references/` tree (local-only, not committed) per the docs
convention. Fetch a local copy from the `modelcontextprotocol/modelcontextprotocol`
repository at `schema/draft/schema.ts` (revision 2026-07-28). The draft moves, so
re-pull before relying on exact shapes:

```bash
mkdir -p docs/references
gh api repos/modelcontextprotocol/modelcontextprotocol/contents/schema/draft/schema.ts \
  --jq '.content' | base64 -d > docs/references/mcp-draft-2026-07-28-schema.ts
```

## Conformance harness

The official conformance suite (`@modelcontextprotocol/conformance`) validates a
running server over Streamable HTTP. Run it locally with:

```bash
just conformance                 # active suite, default port 41060
just conformance active 41170    # explicit suite and port
```

Notes:

- The recipe boots a no-auth loopback server, waits for `/health`, runs the
  suite, and tears down. It defaults to port 41060 to avoid colliding with a live
  server on the default 40060, pre-checks the port is free, and verifies our
  process is the one answering.
- Requires `npx` (Node.js).
- `conformance-baseline.yml` fences known gaps so the recipe fails only on a new
  regression (and flags a baselined scenario that starts passing as stale).
- Current baseline: the core protocol scenarios pass (server-initialize, ping,
  tools-list, resources-list, prompts-list, completion-complete,
  dns-rebinding-protection). Fixture scenarios (the suite calls named `test_*`
  tools/resources/prompts that the generic `example` tool does not expose) and
  optional features (logging, sampling, progress, subscribe, elicitation
  variants, and multi-SSE in json_response mode) are expected failures.

## References

- Draft spec: https://modelcontextprotocol.io/specification/draft
- Draft changelog: https://modelcontextprotocol.io/specification/draft/changelog
- Conformance suite: https://github.com/modelcontextprotocol/conformance
- rmcp (Rust SDK): https://github.com/modelcontextprotocol/rust-sdk
