# crates/integrations

Standalone, per-service API client crates extracted from Jacob's rmcp-server
projects — UniFi, Unraid, Arcane, Apprise, Gotify, Tailscale, ytdl, and
yarr's sub-services (Sonarr, Radarr, Prowlarr, Overseerr, Tautulli,
Tracearr, Plex, SABnzbd, qBittorrent, Jellyfin, Bazarr). Each one wraps a
single external service's API and nothing else — no `rmcp`, no `axum`, no
soma-specific types. They exist so the API-client logic can be reused
outside soma (a CLI, a different MCP server, a plain script) and eventually
published to crates.io independently.

**[`unifi`](unifi) is the reference example.** When extracting the next
service, read it first and copy its shape rather than inventing a new one.

## What belongs here

A crate belongs in `crates/integrations` when:

1. It wraps a specific external service's API (UniFi, Sonarr, Gotify, ...).
2. It has no dependency on `crates/soma/*` or `crates/shared/*` — enforced
   by `cargo xtask check-architecture` (see "Architecture enforcement"
   below). It may depend on another `crates/integrations/*` crate (e.g. a
   shared `arr-common` helper for the yarr family) and on ordinary
   crates.io dependencies.
3. An unrelated Rust project could use it without pulling in anything
   soma-specific.

If a crate needs soma's own types, auth, or runtime state, it doesn't
belong here — that's product-specific integration glue and belongs in
`crates/soma/integrations` instead (a different, already-reserved concept:
bridges from `SomaApplication` ports to concrete shared engines, not
third-party API clients).

## Checklist for a new integration crate

Work through this in order; `unifi` demonstrates every item.

- [ ] **Location and name.** `crates/integrations/<service>/`, package name
      matching the directory (`sonarr`, not `soma-sonarr` — these crates
      are meant to stand alone, so don't carry the `soma-` prefix that
      `crates/shared/*` uses).
- [ ] **Cargo.toml.**
  - `[package.metadata.soma-architecture] layer = "vendor"` — required, or
    `check-architecture` rejects the crate.
  - `publish = false` until the crate is actually ready to ship; flip it
    deliberately, not as a side effect of adding the crate.
  - `readme = "README.md"`.
  - `authors.workspace = true`, `repository.workspace = true` for the
    shared metadata; write a crate-specific `description`, `keywords`,
    `categories`. **Do not** inherit `homepage.workspace = true` — it
    resolves to soma's own product site, which is wrong for a crate that's
    supposed to be usable with zero soma coupling. Omit `homepage` unless
    the wrapped service genuinely has a dedicated homepage worth linking.
  - Bundle a `LICENSE` file **inside the crate's own directory**. The
    workspace-root `LICENSE` does not get packaged automatically —
    `license = "MIT"` alone is enough for `cargo publish` to succeed, but
    the tarball ships with no actual license text unless you copy one in.
    Run `cargo package -p <crate> --list` and confirm `LICENSE` is in the
    file list before considering this done.
- [ ] **Typed errors, not `anyhow`.** Define `error.rs` with a
  `thiserror`-derived enum and a `pub type Result<T> = ...` alias. Every
  public function returns it. Give the HTTP-status-class failures
  (unauthorized, forbidden, not-found, rate-limited, timeout, connect,
  decode) their own variants so callers can `match`/`matches!` instead of
  parsing a message string — that's the entire point of not using
  `anyhow::Error` here. Mark the enum `#[non_exhaustive]`: these crates are
  meant to be published and consumed externally, so adding a variant later
  must not be a semver break for downstream `match` arms.
- [ ] **One pooled HTTP client, reused, with a configurable timeout.**
  Build the `reqwest::Client` once (in the client constructor) and thread a
  reference through every request path — including the dynamic/generic
  action-dispatch path. Do not call `Client::new()`/`ClientBuilder::build()`
  per request; that defeats connection pooling and keep-alive. `unifi`'s
  `http::request_json` / `UnifiClient::request_json` is the pattern: one
  function that takes an already-built `&Client` and maps the response,
  called from everywhere. Put the request timeout on the config struct
  (`UnifiConfig::request_timeout`, with a sane default) rather than a
  hardcoded constant — some services (backups, image pulls, exports)
  legitimately need longer than a typical REST call.
- [ ] **Redact secrets from `Debug`.** If the config or client type holds
  an API key, token, or password, do not `#[derive(Debug)]` it — write a
  manual `impl Debug` that shows only a length/placeholder for the secret
  field. An incidental `tracing::debug!(?config)` or `{:?}` in a log
  statement should never leak a credential.
- [ ] **Tests.**
  - Inline `#[cfg(test)] mod tests` next to any pure logic (path/URL
    building, request normalization, name mapping, response filtering).
    These crates tend to have a lot of this kind of logic; test it — it's
    cheap and it's what actually breaks when a controller's API changes.
  - A `tests/<crate>.rs` integration test using
    [`wiremock`](https://docs.rs/wiremock) to exercise the HTTP client
    against a mock server: one success case, one auth-failure case, one
    not-found case, one rate-limited case, one malformed-response case. No
    real upstream service needed to run `cargo test`.
  - A separate `tests/action_dispatch.rs` (or equivalent) driving the
    dynamic dispatcher end-to-end — capability lookup, path substitution,
    URL construction, HTTP call — through its public entry point, not just
    its individual pieces. `unifi`'s per-request-type tests originally
    covered every named method but never exercised the dispatcher itself;
    that gap let a real routing bug through review undetected. Don't repeat
    that shape.
  - If the crate has a bundled capability/action catalog (a JSON or static
    table describing each dispatchable action's method + path), add a
    catalog-wide invariant test that no mutating action's declared
    method+path collides with a read-only action's. `unifi` shipped with
    21 mutating admin actions that silently re-ran a read instead of
    performing their mutation, all sharing this exact defect shape — a
    single collision-detection test (see
    `capabilities/internal_network.rs`'s
    `no_dispatchable_mutating_action_shares_a_get_path_with_a_read_only_action`)
    catches the whole class instead of one hardcoded regression test per
    incident.
- [ ] **Docs.** Crate-level `//!` docs in `lib.rs` with a quick-start
  example (checked by `cargo test` as a doctest) and a module-layout table.
  `#![deny(missing_docs)]` at the crate root (not `warn` — this is the file
  every future crate copies verbatim, so the "fully documented public API"
  bar should be enforced locally, not borrowed incidentally from CI's
  `-D warnings` flag), with every public item documented. Add `# Errors`
  sections to public `Result`-returning functions.
- [ ] **README.md.** Quick start, module layout, error-handling note,
  testing instructions, publish status. `unifi/README.md` is the template.
- [ ] **Bundled fixture/data files go in the crate's own `data/`
  directory**, not a repo-root path reached via `../../..`. This makes the
  crate self-contained (works if it's ever split into its own repo) and
  sidesteps a real bug we hit once: the repo's `.gitignore` had an
  unanchored `data/` pattern that silently dropped a crate-local `data/`
  directory from git. That's fixed (the pattern is now anchored to the
  repo root), but if you add a new top-level ignore pattern anywhere in
  this repo, anchor it (`/name/`) unless you deliberately want it to match
  every nested directory with that name.

## Architecture enforcement

`xtask/src/architecture_graph.rs` and `xtask/src/architecture.rs` define a
`Layer::Vendor` matched on any `crates/integrations/*` path. The rule:
vendor packages may depend on other vendor packages, but not on anything in
`crates/shared/*` or `crates/soma/*`. Run `cargo xtask check-architecture`
after adding a crate here — it fails loudly (not silently) if the
dependency graph is wrong. This is the same enforcement mechanism the rest
of the workspace uses for its `shared`/`product-*` layers; see
`soma-architecture-refactor-plan-v3.md` at the repo root for the full
layering model (that plan is scoped to soma's own internal crates, though —
this file is the source of truth for `crates/integrations`).
