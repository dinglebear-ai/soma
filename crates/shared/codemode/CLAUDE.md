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

Runner deadlines have two nested control-plane guarantees. External host tool
calls reserve 250ms of an ordinary execution budget, when the budget is large
enough, so a timed-out call can still receive its `ToolError` and acknowledge
with `Done`/`Error`. After a `ToolResult`/`ToolError` is delivered, the parent
arms a 5s settlement watch clipped by the original execution deadline. Any new
runner protocol activity clears that watch. If the outer deadline is earlier or
exactly equal to the settlement deadline, it remains an ordinary execution
timeout; only a genuinely grace-limited expiry is reported as runner settlement.

Local providers are explicit and reserved: `state`, `git`, and, only with the
`openapi` feature, `openapi`. State and git calls may serialize around local
mutable state; OpenAPI dispatch must remain outside that lock.

Use Soma naming for runtime/home configuration: `SOMA_HOME`, `~/.soma`, and
`SOMA_CODE_MODE_*` / `SOMA_CODE_MODE_POOL_*` environment variables. Artifact
writes are bounded per file and per run, and the shared artifact store is pruned
on the first write of each run. The default retention window is 200 runs with a
4 GiB total-store budget; `SOMA_CODE_MODE_ARTIFACT_RETENTION_RUNS=0` disables
count pruning and `SOMA_CODE_MODE_ARTIFACT_MAX_STORE_MIB=0` disables byte
pruning. Active runs must remain protected from concurrent prune passes.

Tests live in sibling `*_tests.rs` files. Do not add inline `mod tests`, `mod.rs`,
or any Rust source/test file over 500 physical lines.
