# Native Service Action Registry Closeout

## Summary

Implemented the native Rust service action registry plan on
`codex/native-service-action-registry` and opened PR #84.

The service crate now owns the action registry and native dispatch path. CLI,
MCP, and REST surfaces derive action metadata and route behavior from that
registry rather than duplicating action lists. REST uses direct typed routes
such as `/v1/greet`, `/v1/echo`, `/v1/status`, and `/v1/help`; the removed
`POST /v1/example` envelope is covered by an integration test.

## Review Follow-Ups

- Kept REST params strict for direct routes and added coverage for unknown
  fields.
- Moved action metadata generation to `rtemplate-service` and updated docs,
  OpenAPI, MCP schema docs, parity tables, scaffold helpers, and pattern checks.
- Added typed `ParamType` metadata so schema generation does not depend on
  free-form type strings.
- Preserved `ParamDoc` max length and enum metadata in MCP schemas.
- Ensured advertised REST action routes are actually mounted.
- Exercised the full MCP `call_tool` validation path for structured errors.
- Kept destructive CLI confirmation on the interactive path and added registry
  driven tests for `--yes` / `-y`.
- Made the service HTTP client dependencies optional so `xtask` does not inherit
  `reqwest`.
- Updated generated-doc parsing so it reads the service-owned action registry and
  understands typed `ParamType` metadata.
- Bumped the shipped template component to `0.4.3` and updated `anyhow` to
  `1.0.103` so CI release/version and Cargo Deny gates pass.
- Touched `scripts/README.md` and `docs/PLUGINS.md` to satisfy the committed
  coupled-file policy for script and plugin-skill generation changes.

## Verification

- `cargo fmt --check`
- `cargo test --workspace --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo xtask check-schema-docs --check`
- `cargo xtask check-openapi --check`
- `cargo xtask check-docs`
- `cargo xtask check-version-sync`
- `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`
- `cargo xtask test-template-features`
- `cargo xtask check-coupled-files origin/main HEAD`
- `cargo xtask check-blob-size --base origin/main --head HEAD`
- `cargo deny check all`
- `git diff --check`
- `cargo tree -p xtask -i reqwest` confirmed `reqwest` is absent from the
  `xtask` dependency graph.
