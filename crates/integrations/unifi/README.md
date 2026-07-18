# unifi

Standalone Rust client for UniFi Network Controllers: authentication, UniFi's
documented official REST API, the controller's internal (undocumented, but
stable) web-UI API, capability discovery, and a dynamic action dispatcher
that routes 244 named actions across both.

This crate has no dependency on `rmcp`, `axum`, or anything soma-specific —
it only knows how to talk to a UniFi controller. It's meant to be embedded by
an MCP server, a CLI, an HTTP handler, or a plain script. See
`crates/integrations/README.md` (one level up) for what makes this a
`crates/integrations/*` crate and the checklist to follow when extracting
the next one — **this crate demonstrates every item on it.**

## Contents

- [Installation](#installation)
- [Quick start](#quick-start)
- [Two ways to use this crate](#two-ways-to-use-this-crate)
- [Configuration](#configuration)
- [The two controller APIs](#the-two-controller-apis)
- [Dynamic action dispatch](#dynamic-action-dispatch)
- [Legacy controller support](#legacy-controller-support)
- [Error handling](#error-handling)
- [Module layout](#module-layout)
- [Extending: adding or fixing a capability](#extending-adding-or-fixing-a-capability)
- [Testing](#testing)
- [Status](#status)

## Installation

```toml
[dependencies]
unifi = { path = "../../crates/integrations/unifi" } # or a version once published
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Runtime dependencies: `reqwest` (rustls-tls, no OpenSSL), `serde`/`serde_json`,
`thiserror`, `tracing`. Dev-only: `wiremock` for HTTP tests. No `unsafe`
code, no proc-macro dependencies beyond `serde_derive`/`thiserror`'s own.

## Quick start

```rust,no_run
use unifi::{UnifiClient, UnifiConfig};

# async fn run() -> Result<(), unifi::UnifiError> {
let client = UnifiClient::new(&UnifiConfig {
    url: "https://unifi.local".to_string(),
    api_key: std::env::var("UNIFI_API_KEY").unwrap_or_default(),
    ..UnifiConfig::default()
})?;

let clients = client.clients().await?;
println!("{clients}");
# Ok(())
# }
```

Generate an API key in the controller's **Settings → Control Plane → Integrations**
(or **Settings → Admins** on older versions) and pass it as `api_key`.
Authentication uses the `X-API-KEY` header on every request.

## Two ways to use this crate

| Type | Use when | Surface |
|---|---|---|
| [`UnifiService`] | You're embedding this crate in an application (CLI, MCP tool, HTTP handler) | 8 named read methods + [`execute`](UnifiService::execute) for anything else. **Depend on this, not `UnifiClient`, unless you have a specific reason not to** — it's the stable seam for adding cross-cutting behavior (result shaping, caching, metrics) without touching every call site. |
| [`UnifiClient`] | You need the HTTP client directly (building a new facade, writing a test, calling `request_json` for something the dispatcher doesn't cover) | The pooled `reqwest::Client`, the same 8 named methods `UnifiService` wraps, plus [`request_json`](UnifiClient::request_json) — the low-level primitive both `UnifiClient`'s named methods and the dynamic dispatcher build on. |

Both types are cheap to `Clone` (an `Arc`-backed `reqwest::Client` under the
hood) — clone freely instead of wrapping in your own `Arc`/`Mutex`.

The 8 named methods, identical on both types:

| Method | Internal action | What it returns |
|---|---|---|
| `clients()` | `clients` | Connected clients (wireless and wired) |
| `devices()` | `devices` | Network devices: APs, switches, gateways |
| `wlans()` | `wlans` | WLAN (WiFi network) configurations |
| `health()` | `health` | Site health summary |
| `alarms()` | `alarms` | Active alarms / alerts |
| `events(limit)` on `UnifiService`; `events()` on `UnifiClient` | `events` | Recent events. `UnifiService::events` truncates to `limit` entries when given; `UnifiClient::events` returns everything the controller sends |
| `sysinfo()` | `sysinfo` | Controller system info |
| `me()` | `me` | Authenticated user info |

These are the eight actions guaranteed stable and directly callable without
going through [`ActionRequest`]/[`ActionDispatcher`] — pick them when you know
at compile time which endpoint you want; use dynamic dispatch (below) for
everything else, or when the action name is only known at runtime (e.g. a
tool call from an LLM).

## Configuration

[`UnifiConfig`] is `Clone + Serialize + Deserialize` (`Debug` is hand-written
to redact `api_key`) with `#[serde(default)]`, so it round-trips through
JSON/YAML/TOML and any field you omit falls back to its default:

| Field | Env var (by convention, not read by this crate) | Default | Notes |
|---|---|---|---|
| `url` | `UNIFI_URL` | `""` | Controller base URL, e.g. `https://unifi.local`. Required — `UnifiClient::new` returns [`UnifiError::MissingUrl`] if empty. |
| `api_key` | `UNIFI_API_KEY` | `""` | Sent as the `X-API-KEY` header. Required — returns [`UnifiError::MissingApiKey`] if empty. |
| `site` | `UNIFI_SITE` | `"default"` | Site slug used by every internal-API per-site path. |
| `skip_tls_verify` | `UNIFI_SKIP_TLS_VERIFY` | `false` | Skip TLS certificate verification. Defaults to verifying — self-signed local controllers need this explicitly set to `true`; the client never silently accepts an invalid certificate. |
| `legacy` | — | `false` | Legacy controller mode: no `/proxy/network` prefix, typically port 8443. See [Legacy controller support](#legacy-controller-support). |
| `request_timeout` | — | 30s ([`DEFAULT_REQUEST_TIMEOUT`]) | Per-request timeout on the pooled `reqwest::Client`. Override for controllers or actions (large exports, slow WAN links) that routinely need longer. `std::time::Duration` has no built-in `serde` support, so this (de)serializes as whole seconds via a hand-written module rather than pulling in `serde_with`/`humantime-serde` for one field. |

This crate does not read environment variables itself — the "env var"
column above documents the convention its own tests and this README use;
an embedder decides how config is sourced (env, file, CLI flags, ...) and
constructs `UnifiConfig` directly.

`UnifiClient::config()` reconstructs the `UnifiConfig` a client was built
from — every field the client stores is threaded through, so this round-trips
exactly, including a non-default `request_timeout`.

## The two controller APIs

UniFi controllers actually expose two REST surfaces, and this crate speaks
both:

| | Official ([`api::official`], [`actions::official`]) | Internal ([`api::internal`], [`actions::internal`]) |
|---|---|---|
| What it is | UniFi's documented `/proxy/network/integration` API | The controller's own (undocumented, but stable in practice) web-UI API — what the UniFi mobile/web app itself calls |
| Source of the capability catalog | `data/unifi_official_network_v10_3_58.json`, generated from UniFi's OpenAPI spec (78 operations, all 5 HTTP verbs) | `data/unifi_internal_endpoint_models.json`, a curated inventory (175 entries; 153 currently dispatchable, 22 disabled — see below) |
| Action naming | `official_` + the OpenAPI `operationId` in `snake_case`, with a handful of curated short-name overrides (`official_list_sites`, `official_list_clients`, `official_list_devices`, `official_list_networks`, `official_list_wifi`, and the five `official_connector_*` actions) | The inventory's own `action` field verbatim (e.g. `unifi_get_lldp_neighbors`), plus 8 hand-written "legacy" aliases (`clients`, `devices`, `wlans`, `health`, `alarms`, `events`, `sysinfo`, `me` — see the table above) |
| URL shape | `{base_url}/proxy/network/integration/v1/...` | `{base_url}/proxy/network/api/s/{site}/...` (v1-style) or `{base_url}/proxy/network/v2/api/site/{site}/...` (v2-style), or a fixed `/proxy/network/api/self` |
| Some things need a site-scoped `siteId`/`{id}` path param | Yes, e.g. `/v1/sites/{siteId}/clients` | The site is baked into the URL prefix, not a path param |

Every dispatchable action carries an `AuthScope` (`Read` or `Admin`) derived
from its HTTP method (official) or its inventory entry (internal).
**This is informational metadata only — nothing in `ActionDispatcher` or
either `actions::*::execute` enforces it.** If your embedder needs to block
non-admin callers from mutating actions, check
`capabilities::find_capability(action).auth_scope` yourself before calling
`execute`; this crate does not gate anything on its own.

### The 22 disabled internal actions

22 entries in `data/unifi_internal_endpoint_models.json` are declared with
`"runtime": false` and are therefore absent from
[`capabilities::all_capabilities`] entirely — dispatching their action name
returns [`UnifiError::UnknownAction`], not a broken response. Each was found
to declare itself `mutating: true` with a `GET` method against a path
identical to an existing read-only listing endpoint (e.g.
`unifi_block_client` declares `GET /rest/user`, the same path
`unifi_list_clients`'s legacy alias uses) — dispatching one would silently
re-run the read and report success without performing the intended mutation.
Each has an `evidence` field explaining what's wrong and what to check before
re-enabling it. A catalog-wide test
(`capabilities::internal_network::tests::no_dispatchable_mutating_action_shares_a_get_path_with_a_read_only_action`)
guards against this bug class recurring, whether from hand-editing the JSON
or regenerating it from a stale source.

## Dynamic action dispatch

All 244 entries in [`capabilities::all_capabilities`] — the 8 named methods
included — are reachable by action name through [`ActionDispatcher`]; it's
the only way to reach the other 236 (78 official + 153 internal + 5 hybrid).
The 22 disabled internal actions (see below) are excluded from that count
entirely — their action names return [`UnifiError::UnknownAction`].

```rust,no_run
use unifi::{ActionDispatcher, ActionRequest, UnifiClient, UnifiConfig};
use serde_json::json;

# async fn run() -> Result<(), unifi::UnifiError> {
let client = UnifiClient::new(&UnifiConfig {
    url: "https://unifi.local".to_string(),
    api_key: std::env::var("UNIFI_API_KEY").unwrap_or_default(),
    ..UnifiConfig::default()
})?;
let dispatcher = ActionDispatcher::new(client);

// A dynamic official-API action with a path parameter.
let clients = dispatcher
    .execute(ActionRequest {
        action: "official_list_clients".to_string(),
        params: json!({ "siteId": "<site-id-from-official_list_sites>" }),
    })
    .await?;
# let _ = clients;
# Ok(())
# }
```

**`ActionRequest::params` shape**: path-template placeholders (e.g.
`{siteId}`, `{device_mac}`) are read from **top-level** keys of `params`;
the HTTP query string and request body are read from the reserved
`params.query` and `params.body` sub-objects respectively. This split is
easy to get backwards — path params are *not* nested under a `path` key.

`ActionDispatcher::execute` resolves an action in one of three ways,
matching [`api::ApiSourceFamily`]:

1. **`Official`** — looked up in the official catalog, path-substituted,
   dispatched via `actions::official::execute`.
2. **`Internal`** — looked up in the internal catalog; the 8 named actions
   (`clients`, `devices`, ...) go through `UnifiClient`'s methods, everything
   else through `actions::internal::execute_generic`'s path substitution +
   dispatch. A few internal actions get their method/path/body silently
   patched before dispatch (`normalize_internal_request` in
   `actions/internal.rs`) because the controller's real web-UI traffic
   doesn't match what the capability catalog would otherwise construct —
   e.g. `events`/`unifi_recent_events`/`unifi_get_ips_events` are actually a
   `POST /v2/system-log/all`, not the `GET` the catalog lists.
3. **`Hybrid`** — 5 actions (`list_clients`, `list_devices`,
   `list_networks`, `list_wifi`, `get_system_info`) that resolve to a
   concrete official or internal action at call time
   ([`actions::hybrid::resolve`]): pass `params.prefer` (`"official"` or
   `"internal"`) to force a side, otherwise the presence of a non-null
   `params.siteId` is taken as a signal you want the official API's
   site-scoped shape. `prefer` is stripped before the resolved request is
   forwarded.

Unknown action names return [`UnifiError::UnknownAction`] before any HTTP
request is attempted.

[`UnifiService::execute`] is a thin wrapper over the same dispatcher for
callers who'd rather depend on one type — see
[Two ways to use this crate](#two-ways-to-use-this-crate).

## Legacy controller support

Some UniFi controllers (older self-hosted installs, typically on port 8443)
don't sit behind the `/proxy/network` reverse-proxy prefix modern UniFi OS
consoles use. Set `UnifiConfig::legacy = true` for these — every internal-API
path builder (`api::internal::InternalNetworkApi::v1_site_path`/`v2_site_path`,
`UnifiClient::site_path`/`self_path`) omits the `/proxy/network` prefix
accordingly. This only affects the internal API; the official API's URL
shape doesn't change based on `legacy`.

## Error handling

Every fallible function returns [`UnifiError`] (aliased as [`Result`]), never
`anyhow::Error` or a boxed `dyn Error`. `UnifiError` is `#[non_exhaustive]`:
a `match` on it must include a wildcard arm, since new variants (a future
status-class split, for instance) can be added without that being a
downstream breaking change.

| Variant | When | Notes |
|---|---|---|
| `MissingUrl` / `MissingApiKey` | `UnifiConfig::url`/`api_key` empty at `UnifiClient::new` | Construction-time only |
| `ClientBuild` | The underlying `reqwest::Client` failed to construct | In practice, only from an invalid TLS configuration |
| `Timeout` / `Connect` / `Request` | Transport failure | All three keep `#[source] reqwest::Error` for `Error::source()` chain-walking |
| `Unauthorized` | HTTP 401 — API key rejected | No `method` field (unlike the other status-class variants): a rejected key is rejected the same way for every verb |
| `Forbidden` | HTTP 403 — key valid, lacks permission | |
| `NotFound` | HTTP 404 | |
| `RateLimited` | HTTP 429 | Carries a parsed `retry_after: Option<Duration>` from the response's `Retry-After` header (seconds form only — the HTTP-date form isn't parsed) |
| `EmptyBody` | A `GET` returned a success status with no body | |
| `Decode` | Response body wasn't valid JSON | Only reachable on a *success* status — a non-success status with a non-JSON body maps to `UnexpectedStatus` with a best-effort-captured body instead, so an HTML error page from a proxy doesn't masquerade as an opaque decode failure |
| `UnexpectedStatus` | Any other non-success status | `body` is JSON when the response was JSON, otherwise the raw text, boxed to keep the enum small |
| `UnknownAction` | `ActionDispatcher::execute` was asked to run an action with no registered capability | |
| `InvalidRequest` | Malformed request parameters (wrong type, wrong API family for the dispatch path taken, ...) | `context` is an action name or `method path`, whichever was on hand |
| `PathTemplate` / `ConnectorPath` / `HybridRouting` | Path-substitution, Connector-wildcard-validation, or hybrid-routing failure respectively | Collapsed to a `String` payload — each is a dispatch/routing/config-integrity failure a caller can't meaningfully retry differently on, not a status a caller needs to branch on |

Match on the variant when a caller needs to react differently — e.g. prompt
for a new API key on `Unauthorized`, back off using `RateLimited`'s
`retry_after`, or retry on `Timeout`/`Connect`.

## Module layout

| Module | Owns |
|---|---|
| `client` / [`UnifiClient`] | The pooled HTTP client and its 8 named, fixed endpoints |
| `service` / [`UnifiService`] | The facade embedders should depend on |
| [`actions`] | Dynamic action dispatch ([`ActionDispatcher`]), split into `actions::official`, `actions::internal`, `actions::hybrid` |
| [`api`] | Path/URL construction for both controller APIs ([`api::ApiSourceFamily`]), split into `api::official`, `api::internal`, and the shared `api::path` (template substitution, connector-wildcard validation) |
| [`capabilities`] | The action catalog ([`capabilities::Capability`], [`capabilities::AuthScope`], [`capabilities::all_capabilities`]/[`capabilities::find_capability`]), built once from the two `data/*.json` inventories, split into `capabilities::official_network` and `capabilities::internal_network` |
| `config` / [`UnifiConfig`] | Connection configuration |
| [`http`] | The one place HTTP requests are actually sent and errors mapped ([`http::build_client`], [`http::request_json`]) |
| [`error`] / [`UnifiError`] | Every typed failure this crate can return |
| `util` | Small internal helpers (`truncate_data_array`) shared by `service` and `actions::internal` |

`client`, `config`, `service`, and `util` are private modules; their public
types (`UnifiClient`, `UnifiConfig`, `DEFAULT_REQUEST_TIMEOUT`,
`UnifiService`) are re-exported at the crate root, alongside `error`'s
`UnifiError`/`Result` and `actions`'s `ActionDispatcher`/`ActionRequest`.
`actions`, `api`, `capabilities`, and `http` are `pub mod` in their own
right (their nested types have no reason to be flattened to the crate root).

## Extending: adding or fixing a capability

- **A new internal-API action**: add an entry to
  `data/unifi_internal_endpoint_models.json` (`action`, `method`, `path`,
  `title`, `mutating`, `runtime: true`, `auth_scope`, `verification_mode`,
  `evidence`) — it's picked up automatically by
  `capabilities::internal_network::capabilities()`. If the controller's real
  request differs from a plain method/path/body substitution (a different
  verb, a synthesized body, a rewritten path), add a case to
  `normalize_internal_request` in `actions/internal.rs` rather than trying
  to encode it in the JSON.
- **A new official-API action**: regenerate
  `data/unifi_official_network_v10_3_58.json` from UniFi's OpenAPI spec (or
  hand-add an operation entry); the action name is derived automatically by
  `capabilities::official_network::action_name` unless you add a curated
  override there.
- **A new hybrid action**: add a `hybrid(...)` entry in
  `capabilities/internal_network.rs` and matching arms in both
  `official_target`/`internal_target` in `actions/hybrid.rs`.
- **Before committing any catalog change**, run `cargo test -p unifi` — the
  catalog-wide invariant test
  (`no_dispatchable_mutating_action_shares_a_get_path_with_a_read_only_action`)
  and the duplicate-action-name test
  (`capabilities::tests::all_capabilities_has_no_duplicate_action_names`)
  both run against the real bundled JSON, not a fixture, so they catch a bad
  entry immediately.

## Testing

- Pure logic (path substitution, request normalization, name mapping, hybrid
  routing, the catalog invariants above) has inline `#[cfg(test)] mod tests`
  next to the code it tests — 63 of these.
- `tests/client.rs` exercises `UnifiClient`'s named methods end-to-end
  against a [`wiremock`](https://docs.rs/wiremock) mock server — no real
  controller needed: one success case, `Unauthorized`/`NotFound`/
  `RateLimited` (with and without `Retry-After`)/`EmptyBody`/
  `UnexpectedStatus` (JSON and non-JSON body) failure cases, plus
  constructor-validation cases. Copy this file's pattern for testing another
  integration crate's HTTP client.
- `tests/action_dispatch.rs` drives `ActionDispatcher::execute` end-to-end —
  capability lookup, hybrid resolution, path substitution, URL construction,
  HTTP call — since that's the crate's main entry point for anything beyond
  the 8 named methods, and per-request-type tests alone don't exercise the
  dispatcher itself. Copy this file's pattern too; testing only the named
  methods leaves the dynamic-dispatch path — most of what makes an
  integration crate's action count large — unverified.

```bash
cargo test -p unifi
cargo clippy -p unifi --all-targets -- -D warnings
cargo doc -p unifi --open
cargo package -p unifi --list   # confirm what would actually ship
```

## Status

Not yet published — `publish = false` in `Cargo.toml`. The package is named
`unifi` for now (matching the crate it was extracted from); pick a
brand-neutral, crates.io-available name before publishing. The bundled
`LICENSE` (MIT) is packaged with the crate independently of the
workspace-root `LICENSE` — `cargo package -p unifi --list` confirms it ships
in the tarball.
