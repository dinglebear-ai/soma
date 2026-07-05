# Issue 83 Provider Runtime Review Closeout

Branch: `codex/issue-83-dynamic-provider-runtime`
PR: https://github.com/jmagar/template-rmcp/pull/87

## Scope

- Implemented the first issue 83 dynamic provider runtime slice: manifest-backed registry, static Rust provider adapter, MCP/REST/CLI/palette/OpenAPI generated surfaces, capability enforcement, and provider contract checks.
- Kept OpenAPIProvider, MCPProvider, WASM, and AI SDK execution behind explicit deferred tests until their isolation and security contracts are implemented.
- Bumped the shipped template component to `0.4.5` because `origin/main` is already tagged `v0.4.4`.

## Review Follow-Up

- Ran Lavra review and PR review toolkit agents.
- Addressed dynamic MCP dispatch bypassing provider actions.
- Mounted dynamic REST provider routes and served runtime OpenAPI from the provider snapshot.
- Enforced scoped capability grants, admin-only provider tools, provider scopes, destructive confirmation, request limits, and compiled input schemas.
- Preserved provider error code/provider/action/remediation in structured MCP/REST errors.
- Fixed mounted REST missing-auth fallback to use an anonymous zero-scope principal.
- Redacted sensitive environment variable values from public provider diagnostics.
- Aligned provider manifest schema and Rust DTOs with the implemented surface.
- Added generated provider/palette drift checks to xtask and CI.

## Verification

- `cargo fmt --all --check`
- `cargo test --workspace --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo xtask check-openapi-drift`
- `cargo xtask check-schema-docs --check`
- `cargo xtask check-provider-manifest-contract`
- `cargo xtask check-palette-manifest --check`
- `cargo xtask check-version-sync`
- `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`
- `git diff --check`
