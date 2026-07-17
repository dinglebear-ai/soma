// Structural sibling for wasm.rs. Executable WASM behavior needs a compiled
// fixture module and is covered end-to-end by soma-service's
// `apps/soma/tests/wasm_provider.rs` and `drop_provider_probe.rs`, which
// drive this crate's `WasmProvider` through soma-service's drop-in provider
// directory loader.
