# Reference Soma component provider

Build with the component-model target:

```bash
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2
```

The resulting `.wasm` implements `soma:provider@1.0.0`. To hot-drop it, copy
the artifact and the checked-in manifest with matching stems:

```bash
cp target/wasm32-wasip2/debug/soma_reference_component_provider.wasm \
  providers/reference-provider.wasm
cp provider.wasm.json providers/reference-provider.wasm.json
```

Soma detects the component encoding and keeps the existing core-Wasm ABI as a
compatibility fallback. `../conformance-v1.json` is shared with
`../reference-python.py`.
