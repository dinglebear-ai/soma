# Changelog

All notable changes to this crate will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
This crate is not yet published to crates.io (`publish = false`) —
everything below is still `[Unreleased]`; it becomes `[0.1.0] - YYYY-MM-DD`
on first publish.

## [Unreleased]

### Added

- Initial extraction from `gotify-rmcp`'s `src/gotify.rs` (client) and
  `src/app.rs` (service layer): `GotifyClient` (pooled HTTP client, 13 named
  methods covering health/version/messages/applications/clients/current
  user) and `GotifyService` (the facade embedders should depend on, adding
  client-side message text search and application name filtering on top of
  the raw client).
- Typed `GotifyError` (`thiserror`-derived, `#[non_exhaustive]`) in place of
  the original's `anyhow::Result` throughout — distinct variants for
  missing URL, missing client/app token, unauthorized, not found,
  rate-limited (with parsed `Retry-After`), unexpected status, decode
  failure, and the three transport-failure classes (timeout/connect/other),
  each keeping `#[source]` for `Error::source()` chain-walking.
- `GotifyConfig::request_timeout`, configurable, default 30s
  (`DEFAULT_REQUEST_TIMEOUT`), with hand-written `serde` support since
  `std::time::Duration` has none built in.
- `tracing` instrumentation on the one shared `http::request_json` function
  every method call funnels through, so every one of the 13 operations is
  observable — not just a subset, and not duplicated per call site.
- `tests/client.rs`: wiremock HTTP tests covering success, `Unauthorized`,
  `NotFound`, `RateLimited` (with and without `Retry-After`),
  `UnexpectedStatus` (JSON and non-JSON body), the `204 No Content`
  delete-endpoint shape, and both missing-token error paths. None of this
  existed for `GotifyClient`/`GotifyService` in the original project.
- Per-crate `LICENSE` (MIT) and `CHANGELOG.md` (this file).

### Changed

- `GotifyClient::new` now fails fast with `GotifyError::MissingUrl` on an
  empty URL, instead of the original's behavior of logging a warning and
  deferring the failure to the first request. Deliberate behavior change,
  matching `unifi`'s established "construct fast-fails on what every call
  needs" pattern — an empty URL can never succeed, so there's no reason to
  defer discovering that.
- Client and app tokens are still validated lazily (only by the specific
  calls that need them), preserving the original's genuinely useful
  partial-configuration support — a client with only an app token, or none
  at all, remains valid to construct.

### Removed relative to the original service (not bugs — architectural scope cuts)

- **Request/error counters and `/status` (uptime, pid, counters, upstream
  reachability) reporting.** Server-level observability, not a property of
  a Gotify HTTP client — an embedder's own server layer owns this, the same
  way `unifi`'s crate doesn't report soma's own process metrics.
- **The `confirm`/`GOTIFY_ALLOW_DESTRUCTIVE` destructive-action gate.**
  Product/tool-layer UX policy about how an embedder chooses to expose
  `delete_*`, not a client-library concern — see the README's
  [Two ways to use this crate](README.md#two-ways-to-use-this-crate)
  section for the full reasoning (mirrors `unifi`'s `AuthScope`: metadata
  the crate could report, never something it enforces itself).
- **Dynamic action dispatch / capability catalog.** Deliberately never
  built in the first place, not removed — see the README's
  [Why no dynamic action dispatch](README.md#why-no-dynamic-action-dispatch)
  section. Gotify's 13-operation, upstream-fixed API surface doesn't need
  the machinery `unifi`'s 244-action, two-upstream-API surface does.
