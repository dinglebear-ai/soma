# Service Surface Suggestions

## Native Rust Action Add Workflow

To add a native Rust action:

1. Add the business method to `crates/rtemplate-service/src/app.rs` or a focused service module.
2. Add one action metadata entry and one executor match arm in `crates/rtemplate-service/src/actions.rs`.
3. Run `cargo test -p rtemplate-service -p rtemplate-cli -p rmcp-template --tests`.
4. Run `cargo xtask check-openapi --write`.

No edits should be required in `crates/rtemplate-api`, `crates/rtemplate-cli`, or `crates/rtemplate-mcp`.

Future runtime providers such as Wasm or TypeScript should stay deferred until
there is a second concrete provider. The current template intentionally uses a
native Rust registry only.
