# unifi

Standalone client for UniFi Network Controllers: authentication, both the
official and internal REST APIs, capability discovery, and dynamic action
dispatch.

This crate has no dependency on `rmcp`, `axum`, or anything soma-specific —
it only knows how to talk to a UniFi controller. See
`crates/integrations/README.md` (one level up) for what makes this a
`crates/integrations/*` crate and the checklist to follow when extracting
the next one.

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

Most embedders should build on [`UnifiService`] rather than [`UnifiClient`]
directly — it's the stable seam for adding cross-cutting behavior (result
shaping, caching, metrics) without touching every call site.

## Layout

| Module | Owns |
|---|---|
| `client` / `UnifiClient` | The pooled HTTP client and its named, fixed endpoints (`clients`, `devices`, `wlans`, ...) |
| `service` / `UnifiService` | The facade embedders should depend on |
| `actions` | Dynamic action dispatch (`ActionDispatcher`) driven by the capability catalog |
| `api` | Path/URL construction for the official and internal APIs |
| `capabilities` | The action catalog, built from the JSON inventories in `data/` |
| `config` / `UnifiConfig` | Connection configuration |
| `http` | The one place HTTP requests are made and errors mapped |
| `error` / `UnifiError` | Every typed failure this crate can return |

## Error handling

Every fallible function returns [`UnifiError`] (aliased as `Result`), never
`anyhow::Error` or a boxed `dyn Error`. Match on the variant when a caller
needs to react differently — e.g. prompt for a new API key on
`UnifiError::Unauthorized`, back off using `retry_after` on
`UnifiError::RateLimited`, versus retry on `UnifiError::Timeout`.
`UnifiError` is `#[non_exhaustive]`: a `match` must include a wildcard arm,
since new variants can be added without that being a breaking change.

## Configuration

`UnifiConfig::request_timeout` controls the pooled HTTP client's per-request
timeout (default 30s, `unifi::DEFAULT_REQUEST_TIMEOUT`). Override it for
controllers or actions that routinely need longer.

## Testing

- Pure logic (path substitution, request normalization, name mapping, hybrid
  routing) has inline `#[cfg(test)] mod tests` next to the code it tests.
- `tests/client.rs` exercises the HTTP layer end-to-end against a
  [`wiremock`](https://docs.rs/wiremock) mock server — no real controller
  needed. Copy that file's pattern for testing another integration crate's
  HTTP client.
- `tests/action_dispatch.rs` drives the dynamic dispatcher
  (`ActionDispatcher::execute`) end-to-end — capability lookup, hybrid
  resolution, path substitution, URL construction, HTTP call — since that's
  the crate's main entry point for anything beyond the named convenience
  methods, and per-request-type tests alone don't exercise it.
- `capabilities::internal_network::tests::no_dispatchable_mutating_action_shares_a_get_path_with_a_read_only_action`
  is a catalog-wide invariant test: no mutating action's declared
  method+path may collide with a read-only action's (the shape of a real
  defect found in this crate's own bundled data — see git history). Any
  crate with a bundled action catalog should have the equivalent.

```bash
cargo test -p unifi
cargo clippy -p unifi --all-targets -- -D warnings
cargo doc -p unifi --open
```

## Status

Not yet published — `publish = false` in `Cargo.toml`. The package is named
`unifi` for now (matching the crate it was extracted from); pick a
brand-neutral, crates.io-available name before publishing.
