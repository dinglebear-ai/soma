---
title: "MCP Draft Spec (2026-07-28) Compatibility"
created: 2026-06-21
updated: 2026-08-18
---

# MCP Draft Spec (2026-07-28) Compatibility

## Status

Implemented against `rmcp = 3.1.0`.

The requested draft URL currently identifies the integrated protocol revision as
`2026-07-28`. Soma therefore negotiates and tests that exact revision rather than
hard-coding the earlier working-date label. The migration retains compatibility
with older MCP peers where the SDK can prove that a legacy lifecycle is required.

## Runtime compatibility

| Protocol area | Soma status |
|---|---|
| Stateless Streamable HTTP lifecycle | Implemented. Modern HTTP requests do not create or require `Mcp-Session-Id`. |
| `server/discover` | Implemented and exercised over a real TCP/HTTP round trip. |
| Per-request `_meta` identity, version, and capabilities | Implemented by the rmcp 3 client lifecycle and validated by the server. |
| Legacy `initialize` fallback | Retained for older upstream servers through `ClientLifecycleMode::Auto`; modern discovery is attempted first. |
| `resultType` discriminators | Preserved for tools, prompts, resources, task results, and discovery responses. |
| Multi-round-trip requests (`input_required`) | Implemented through the upstream pool, gateway, application port, product integration, and public MCP server. `inputResponses` and opaque `requestState` survive every proxy layer. |
| Tool annotations | Implemented end to end for routed upstream tools. The advertised annotation block, including partial/empty blocks and its absence, is carried separately from Soma's internal safety verdict and re-projected without inventing missing hints. |
| Tasks extension (`io.modelcontextprotocol/tasks`) | Implemented for routed upstream tools. Soma rewrites native task IDs to opaque, subject-bound gateway task IDs and routes `tasks/get`, `tasks/update`, and `tasks/cancel`. |
| `subscriptions/listen` | Implemented with authentication, acknowledgement, filtering, and cancellation. Soma currently advertises no change-notification producers, so the accepted filter is empty instead of claiming events it cannot emit. |
| Modern resource-not-found error semantics | Delegated to rmcp's negotiated-version handling. |
| Capability extensions | Implemented. Task-capable clients and the Soma server advertise the tasks extension explicitly. |
| Discovery/result caching hints | Supported by rmcp models. Soma's discovery response remains private and non-cacheable by default; no broader cacheability claim is made. |
| Authorization updates | Implemented. Soma emits and validates RFC 9207 `iss`, binds persisted credentials and dynamic registrations to the authorization-server issuer, serves Client ID Metadata Documents, accepts published CIMD auth-method sets, supports SSRF-hardened cached `jwks_uri` for `private_key_jwt`, prefers CIMD in automatic mode, retains DCR fallback with `application_type: web`, and exposes a public one-time upstream OAuth callback. |
| MCP protocol headers and CORS | MCP protocol and routing headers are allowed by the HTTP surface and exercised by modern raw-request tests. Outbound Streamable HTTP derives body-truth `Mcp-Method`/`Mcp-Name`, preserves schema-derived `Mcp-Param-*`, and self-heals one typed `HEADER_MISMATCH` with a bounded schema refresh plus one replay. |

## Implementation notes

### Modern client lifecycle

Production upstream connections use discovery-first negotiation for HTTP, stdio,
WebSocket, and OAuth-authenticated transports. Preferred versions are taken from
`ProtocolVersion::KNOWN_VERSIONS` in newest-first order. Legacy initialization is
used only after the peer returns method-not-found for `server/discover`.

Outgoing pooled and manifest-driven clients advertise the tasks extension. The
manifest-driven provider polls asynchronous tasks to a terminal state within its
existing provider timeout. A task that requires interactive input returns an
explicit provider interaction error because that provider surface has no client UI
through which to satisfy elicitation, sampling, or roots requests.

### SEP-2243 routing-header recovery

For Streamable HTTP, Soma treats the serialized JSON-RPC message as the source
of truth for SEP-2243 routing headers. Immediately before sending a request, the
HTTP adapter removes any precomputed `Mcp-Method` and `Mcp-Name` copies and
rebuilds exactly one of each from the body. Schema-derived `Mcp-Param-*` headers
remain untouched. `Mcp-Name` uses the SEP-2243 Base64 sentinel when a raw name
cannot safely round-trip through an HTTP field value.

A server can still reject `tools/call` with the typed JSON-RPC
`HEADER_MISMATCH` code when its tool schema changed after the client's cached
header metadata was populated. Soma recognizes only that code, not message
text. It performs one bounded `tools/list` refresh using the configured
tools-list byte cap, then replays the original call exactly once. A second
header mismatch is surfaced unchanged, preventing an unbounded retry loop.

### Authorization and client registration

Soma's inbound authorization server advertises and emits RFC 9207 `iss` on
successful and failed authorization responses. Outbound upstream authorization
persists the expected issuer and whether the upstream advertised issuer-response
support in the one-time OAuth state row. The public
`/auth/upstream/callback` endpoint recovers the upstream and subject from that
opaque state, forwards the optional `iss` to rmcp, and validates it before the
authorization code is redeemed. Provider errors and malformed callbacks consume
the pending state and return static browser-safe responses without reflecting
codes, state values, tokens, or provider descriptions.

Durable upstream credentials and dynamic client registrations are bound to the
canonical authorization-server issuer. Legacy rows without an issuer and rows
whose issuer no longer matches discovery are deleted and force a fresh
authorization or registration flow. The issuer is also part of the encrypted
credential associated data, so a row cannot be transplanted between issuers.

The `auto` registration strategy follows the draft preference order: use Soma's
served Client ID Metadata Document when the authorization server advertises CIMD
support, otherwise fall back to RFC 7591 Dynamic Client Registration. Explicit
preregistered, explicit CIMD, and explicit dynamic strategies remain available for
operator control and compatibility. DCR requests identify Soma as a web client via
`application_type: web`.

### Multi-round-trip proxying

Soma uses protocol-neutral outcome and continuation types between architectural
layers. rmcp types are converted only at MCP transport boundaries. This prevents
application and domain crates from depending on the SDK while preserving these
wire outcomes exactly:

- complete tool, prompt, and resource results
- `input_required` with keyed input requests
- opaque `requestState`
- keyed `inputResponses` on retries
- task handles

Malformed upstream result objects fail with structured proxy errors instead of
being reported as successful structured content. Likewise, a completed upstream
`tools/call` with `isError: true` is never promoted to success merely because it
contains `structuredContent`. The plain gateway/client path classifies it as a
tool-execution failure, distinct from connection or JSON-RPC transport failure.
Recognized tool-level kinds such as `invalid_param` may survive classification;
unknown or infrastructure-looking upstream kinds collapse to `tool_error`, and
the retained cause is bounded. Plain tool-execution failures also carry a
versioned `details` object for agent course correction: the canonical and
original kinds, optional `retry_after_ms`, advisory `read_only` / destructive /
idempotent / open-world hints, a recovery action, whether unchanged arguments
are conditionally safe, discouraged, or forbidden, human guidance, and a
side-effect posture. Only `retry_later` failures are marked automatically
retryable; `revise_and_retry` requires changing the call first. Raw upstream
content blocks are deliberately not copied into public recovery details, so the
error path cannot become an unbounded or secret-bearing output channel. Before
public exposure, the bounded cause is passed through Soma's standard diagnostic
redactor, and arbitrary unknown `original_kind` strings are discarded rather
than reflected. Safety hints are advisory only and never bypass Soma
authorization or confirmation.
Multi-round-trip relays keep the complete MCP result unchanged so downstream
MCP clients still receive the protocol-native `isError` result.

### Tool annotations and gateway safety

Routed upstream tools retain their MCP `annotations` block independently from
Soma's internal `destructive` gate. A full, partial, or explicitly empty block
is relayed as such; an upstream that advertises no annotation block remains
unannotated downstream. Soma does not manufacture wire hints from its internal
safety decision. This matters for chained gateways because inserting a hint the
upstream did not publish can change a downstream gateway's authorization or
confirmation behavior.

The internal safety verdict intentionally fails closed. An upstream tool is
treated as non-destructive only when it explicitly publishes
`destructiveHint: false` or publishes `readOnlyHint: true` without an overriding
`destructiveHint`. Missing annotations and annotation blocks that specify
neither safety hint remain destructive for Soma's confirmation gates.

`ToolAnnotations` is serialized at the rmcp boundary rather than copied field by
field, so annotation fields added by the rmcp model flow through the neutral
descriptor automatically. As with every SDK-backed implementation, fields that
a future wire peer sends but the installed rmcp version itself does not parse
cannot be preserved until the SDK understands them.

### Task routing and isolation

Native task IDs are scoped to an upstream server and may collide. The gateway
therefore creates opaque public IDs and stores the originating upstream, native ID,
and authorization subject. A task ID cannot be resolved by another subject, and
gateway reload invalidates all in-memory task routes. Soma does not persist task
routes across process restarts.

### Subscriptions

Soma accepts the modern `subscriptions/listen` lifecycle and relies on rmcp to
intersect a requested filter with the server's advertised notification
capabilities. The request stays open until cancellation and returns the draft's
graceful completion result when the server closes it. No synthetic resource,
tool-list, prompt-list, or resource-update events are emitted.

### Deliberately absent protocol areas

Roots, sampling, and logging are not newly added by this migration. They are
deprecated on the draft track and remain absent unless a separate product
requirement justifies them. The older manifest-driven provider transport stack is
still tracked for consolidation into the pooled gateway client, but it now uses the
same rmcp 3 lifecycle and task semantics and is not a protocol compatibility gap.

## Verification

The migration is covered by real transport and routing tests, including:

- modern stateless HTTP discovery with negotiated protocol `2026-07-28`
- absence of `Mcp-Session-Id` on modern requests
- typed complete results
- a two-round elicitation exchange that echoes `requestState` and keyed responses
- live task creation, input-required polling, update, completion, and cancellation
- opaque gateway task IDs, subject isolation, invalid-result rejection, and reload invalidation
- task operations through Soma's public MCP server surface
- modern subscription acknowledgement and cancellation over HTTP
- RFC 9207 issuer state persistence and callback forwarding
- issuer-bound credential rejection, deletion, and reauthorization behavior
- public upstream callback error handling and generated CIMD metadata
- CIMD-first automatic registration with DCR fallback and explicit-strategy preservation
- legacy upstream initialization fixtures
- the bare MCP feature profile and architecture boundaries

Validated commands:

```bash
cargo check --workspace --all-targets
cargo test --workspace --all-targets
```

The full workspace test suite passes. When Soldr's daemon is unavailable, put the
real Rust toolchain directory first in `PATH` so architecture tests that spawn
`cargo` inherit the direct toolchain rather than the Soldr cargo shim.

## Conformance harness

The official conformance suite (`@modelcontextprotocol/conformance`) validates a
running server over Streamable HTTP. Run it locally with:

```bash
just conformance
just conformance active 41170
just conformance-report
```

The recipe boots a no-auth loopback server, enables Soma's conformance fixtures,
runs the upstream suite, and tears the server down. The committed baseline remains
a regression fence and should be refreshed separately when the upstream suite or
draft schema changes. Do not add deprecated roots, sampling, or logging behavior
merely to satisfy legacy conformance cases.

## References

- Draft specification: https://modelcontextprotocol.io/specification/draft
- Draft changelog: https://modelcontextprotocol.io/specification/draft/changelog
- Rust SDK: https://github.com/modelcontextprotocol/rust-sdk
- Conformance suite: https://github.com/modelcontextprotocol/conformance
