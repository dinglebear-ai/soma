# soma-codemode

This crate is a standalone Soma port of Lab Code Mode. It must depend only on
external crates when built with `--no-default-features`; no Lab crate and no
existing Soma crate may appear in that feature graph.

The optional `openapi` feature is the only permitted edge to `soma-openapi`.
Keep all OpenAPI imports, host extension points, JS shims, and dispatch helpers
behind `#[cfg(feature = "openapi")]`; no-feature callers should see normal
unknown-provider behavior for `openapi::*`.

The runner model is Javy/QuickJS with a newline-framed parent/runner protocol.
Do not add ambient Node, filesystem, process, fetch, or network globals to the
sandbox. The standalone runner binary is `soma-codemode-runner`; resolver
overrides use `SOMA_CODE_MODE_RUNNER_EXE`.

Local providers are explicit and reserved: `state`, `git`, and, only with the
`openapi` feature, `openapi`. State and git calls may serialize around local
mutable state; OpenAPI dispatch must remain outside that lock.

Use Soma naming for runtime/home configuration: `SOMA_HOME`, `~/.soma`, and
`SOMA_CODE_MODE_*` / `SOMA_CODE_MODE_POOL_*` environment variables.

Tests live in sibling `*_tests.rs` files. Do not add inline `mod tests`, `mod.rs`,
or any Rust source/test file over 500 physical lines.
