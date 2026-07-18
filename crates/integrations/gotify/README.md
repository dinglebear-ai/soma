# gotify

Standalone Rust client for [Gotify](https://gotify.net) push-notification
servers: sending and managing messages, applications, and clients.

This crate has no dependency on `rmcp`, `axum`, or anything soma-specific —
it only knows how to talk to a Gotify server. It's meant to be embedded by
an MCP server, a CLI, an HTTP handler, or a plain script. See
`crates/integrations/README.md` (one level up) for what makes this a
`crates/integrations/*` crate and the checklist to follow when extracting
the next one; see [`unifi`](../unifi) for the reference example this crate
follows.

## Contents

- [Installation](#installation)
- [Quick start](#quick-start)
- [Two ways to use this crate](#two-ways-to-use-this-crate)
- [Configuration](#configuration)
- [Token types](#token-types)
- [Error handling](#error-handling)
- [Lints and panics](#lints-and-panics)
- [Module layout](#module-layout)
- [Why no dynamic action dispatch](#why-no-dynamic-action-dispatch)
- [Testing](#testing)
- [Status](#status)

## Installation

```toml
[dependencies]
gotify = { path = "../../crates/integrations/gotify" } # or a version once published
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Runtime dependencies: `reqwest` (rustls-tls, no OpenSSL), `serde`/`serde_json`,
`thiserror`, `tracing`. Dev-only: `wiremock` for HTTP tests. No `unsafe`
code.

## Quick start

```rust,no_run
use gotify::{GotifyClient, GotifyConfig};

# async fn run() -> Result<(), gotify::GotifyError> {
let client = GotifyClient::new(&GotifyConfig {
    url: "https://gotify.example.com".to_string(),
    app_token: std::env::var("GOTIFY_APP_TOKEN").unwrap_or_default(),
    ..GotifyConfig::default()
})?;

client.send_message("Build finished", Some("CI"), Some(5), None).await?;
# Ok(())
# }
```

Create an **app token** under **Applications** in the Gotify web UI for
sending messages, and a **client token** under **Clients** for everything
else (listing/deleting messages, managing applications and clients, current
user). These are two distinct token kinds — see
[Token types](#token-types).

## Two ways to use this crate

| Type | Use when | Surface |
|---|---|---|
| [`GotifyService`] | You're embedding this crate in an application (CLI, MCP tool, HTTP handler) | The same 13 operations as `GotifyClient`, plus client-side text search on messages and name filtering on applications — **depend on this, not `GotifyClient`, unless you have a specific reason not to**. |
| [`GotifyClient`] | You need the HTTP client directly (building a new facade, writing a test) | The pooled `reqwest::Client` and all 13 named methods, with no filtering/pagination shaping on top. |

Both types are cheap to `Clone` (an `Arc`-backed `reqwest::Client` under the
hood) — clone freely instead of wrapping in your own `Arc`/`Mutex`.

The 13 methods, identical on both types except where noted:

| Method | Token required | What it does |
|---|---|---|
| `health()` | none | Server health check |
| `version()` | none | Server version |
| `me()` | client | Authenticated user info |
| `messages(app_id, limit, since)` on `GotifyClient`; `messages(app_id, limit, since, offset, query)` on `GotifyService` | client | Lists messages. `GotifyService` adds offset pagination and a case-insensitive text search over `message`/`title` |
| `send_message(message, title, priority, extras)` | app | Sends a notification |
| `delete_message(id)` | client | Deletes one message |
| `delete_all_messages()` | client | Deletes every message |
| `applications()` on `GotifyClient`; `applications(name_filter)` on `GotifyService` | client | Lists applications. `GotifyService` adds a case-insensitive substring filter on `name` — Gotify's own API has no filter parameter here |
| `create_application(name, description, default_priority)` | client | Creates an application |
| `update_application(app_id, name, description, default_priority)` | client | Updates an application |
| `delete_application(app_id)` | client | Deletes an application |
| `clients()` | client | Lists clients |
| `create_client(name)` | client | Creates a client |
| `delete_client(client_id)` | client | Deletes a client |

**This crate does not gate destructive actions behind a confirmation flag.**
The original service this was extracted from added a `confirm`/
`GOTIFY_ALLOW_DESTRUCTIVE` gate at its MCP tool layer — that's product
policy about how *your* tool chooses to expose `delete_*`, not something a
reusable client should bake in. If your embedder wants that, implement it
at your own call sites, the same way `unifi`'s `AuthScope` is metadata the
crate reports but never enforces.

## Configuration

[`GotifyConfig`] is `Clone + Serialize + Deserialize` (`Debug` is
hand-written to redact both tokens) with `#[serde(default)]`, so it
round-trips through JSON/YAML/TOML and any field you omit falls back to its
default:

| Field | Env var (by convention, not read by this crate) | Default | Notes |
|---|---|---|---|
| `url` | `GOTIFY_URL` | `""` | Server base URL, e.g. `https://gotify.example.com`. Required — `GotifyClient::new` returns [`GotifyError::MissingUrl`] if empty. |
| `client_token` | `GOTIFY_CLIENT_TOKEN` | `""` | Management-operation token. Empty means unconfigured — calls that need it return [`GotifyError::MissingClientToken`], not a construction failure (see below). |
| `app_token` | `GOTIFY_APP_TOKEN` | `""` | Send-only token. Empty means unconfigured — [`GotifyClient::send_message`] returns [`GotifyError::MissingAppToken`]. |
| `request_timeout` | — | 30s ([`DEFAULT_REQUEST_TIMEOUT`]) | Per-request timeout on the pooled `reqwest::Client`. |

This crate does not read environment variables itself — the "env var"
column documents the convention this README and its tests use; an embedder
decides how config is sourced and constructs `GotifyConfig` directly.

**Only `url` is validated at construction.** Unlike `unifi`'s single
`api_key` (required upfront), Gotify's two token kinds are each validated
lazily, only by the specific calls that need them — a client built with no
tokens at all is legitimate (`health`/`version` still work with zero
configuration), and one built with only an app token can send without ever
needing a client token. Failing fast on all credentials upfront would make
that legitimate partial-configuration case impossible.

`GotifyClient::config()` reconstructs the `GotifyConfig` a client was built
from — every field the client stores is threaded through, so this
round-trips exactly.

## Token types

Gotify has two independent token kinds, and mixing them up is the most
common integration mistake:

- **Client token** (`GOTIFY_CLIENT_TOKEN`) — created under **Clients** in
  the Gotify web UI. Used for every management operation: listing/deleting
  messages, applications, clients, and current-user lookup.
- **App token** (`GOTIFY_APP_TOKEN`) — created under **Applications**.
  Used *only* for [`send_message`](GotifyClient::send_message). An app
  token cannot list or delete anything; a client token cannot send.

Each error variant that requires a specific token
([`GotifyError::MissingClientToken`], [`GotifyError::MissingAppToken`])
names which one and where to create it, so a misconfiguration is
diagnosable from the error message alone.

## Error handling

Every fallible function returns [`GotifyError`] (aliased as [`Result`]),
never `anyhow::Error` or a boxed `dyn Error`. `GotifyError` is
`#[non_exhaustive]`: a `match` on it must include a wildcard arm, since new
variants can be added without that being a downstream breaking change.

| Variant | When | Notes |
|---|---|---|
| `MissingUrl` | `GotifyConfig::url` empty at `GotifyClient::new` | Construction-time only |
| `MissingClientToken` / `MissingAppToken` | The called operation needs a token that isn't configured | Checked lazily per-call, not at construction — see [Configuration](#configuration) |
| `ClientBuild` | The underlying `reqwest::Client` failed to construct | |
| `Timeout` / `Connect` / `Request` | Transport failure | All three keep `#[source] reqwest::Error` for `Error::source()` chain-walking |
| `Unauthorized` | HTTP 401 — token rejected | |
| `NotFound` | HTTP 404 | |
| `RateLimited` | HTTP 429 | Carries a parsed `retry_after: Option<Duration>` from the response's `Retry-After` header (seconds form only) |
| `UnexpectedStatus` | Any other non-success status | `body` is JSON when the response was JSON, otherwise the raw text, boxed to keep the enum small |
| `Decode` | Response body wasn't valid JSON on a success status | A `204 No Content` (Gotify's delete endpoints) is handled before this point and never reaches it |

Match on the variant when a caller needs to react differently — e.g. prompt
for a new app token on `MissingAppToken` vs. a new client token on
`MissingClientToken`, or retry on `Timeout`/`Connect`.

## Lints and panics

`lib.rs` sets the same three crate-level attributes as `unifi`:
`#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, and
`#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]`
(scoped to non-test builds — `.unwrap()`/`.expect()` in test code is normal,
correct Rust). Unlike `unifi`, which bundles two JSON data files parsed at
build time (and therefore needs a handful of narrowly-`#[allow]`'d panics
for a genuinely-unparseable build), this crate has no bundled data to parse
— so the deny currently has zero exceptions.

## Module layout

| Module | Owns |
|---|---|
| `client` / [`GotifyClient`] | The pooled HTTP client and its 13 named methods |
| `service` / [`GotifyService`] | The facade embedders should depend on |
| `config` / [`GotifyConfig`] | Connection configuration |
| [`http`] | The one place HTTP requests are actually sent and errors mapped ([`http::build_client`], [`http::request_json`]) |
| [`error`] / [`GotifyError`] | Every typed failure this crate can return |

`client`, `config`, and `service` are private modules; their public types
(`GotifyClient`, `GotifyConfig`, `DEFAULT_REQUEST_TIMEOUT`, `GotifyService`)
are re-exported at the crate root, alongside `error`'s
`GotifyError`/`Result`. `http` is `pub mod` in its own right.

## Why no dynamic action dispatch

`unifi` has an `ActionDispatcher`/`Capability` catalog because it fronts
244 actions across two upstream APIs — hand-writing that many named methods
would be unmaintainable, so it builds the catalog from bundled JSON
inventories and resolves an action name to a method+path template at
runtime. Gotify's entire API surface is 13 operations, permanently fixed by
the upstream project (Gotify does not add new endpoints often, and this
crate doesn't need to discover them dynamically). Building a capability
catalog, path-template substitution, and a dispatcher for 13 fixed,
hand-writable methods would be pure ceremony — the flat `GotifyClient`
above *is* the complete, idiomatic shape for a small, stable API. Don't
reach for `unifi`'s dispatch machinery by default when extracting the next
service; reach for it only when the upstream API is large and/or
open-ended enough that hand-writing every method stops being maintainable.

## Testing

- Pure logic (config redaction, timeout round-tripping, message/application
  filtering) has inline `#[cfg(test)] mod tests` next to the code it tests.
- `tests/client.rs` exercises `GotifyClient`'s methods end-to-end against a
  [`wiremock`](https://docs.rs/wiremock) mock server — no real server
  needed: one success case, `Unauthorized`/`NotFound`/`RateLimited` (with
  and without `Retry-After`)/`UnexpectedStatus` (JSON and non-JSON body)
  failure cases, the `204 No Content` delete-endpoint shape, and
  missing-token cases for both token kinds.

```bash
cargo test -p gotify
cargo clippy -p gotify --all-targets -- -D warnings
cargo doc -p gotify --open
cargo package -p gotify --list   # confirm what would actually ship
```

## Status

Not yet published — `publish = false` in `Cargo.toml`. The package is named
`gotify` for now (matching the crate it was extracted from) — check
crates.io name availability and pick a brand-neutral alternative if needed
before publishing, the same way `unifi` had to. The bundled `LICENSE` (MIT)
is packaged with the crate independently of the workspace-root `LICENSE` —
`cargo package -p gotify --list` confirms it ships in the tarball. See
[`CHANGELOG.md`](CHANGELOG.md) for what's changed since extraction;
everything so far is still under `[Unreleased]`.
