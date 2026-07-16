# soma-provider-core

Transport-neutral provider contracts, catalogs, validation, and immutable
registry dispatch for Soma-derived runtimes and standalone provider hosts.

The crate owns provider metadata and generic execution mechanics. It does not
depend on Soma product crates and does not implement product authorization,
configuration, process lifecycle, transport DTOs, or concrete provider
adapters.

```rust,no_run
use soma_provider_core::{ProviderCall, ProviderRegistry};
use serde_json::json;

# async fn example(registry: ProviderRegistry) -> Result<(), Box<dyn std::error::Error>> {
let output = registry
    .dispatch(ProviderCall::new("echo", json!({ "message": "hello" })))
    .await?;
println!("{}", output.into_value());
# Ok(())
# }
```

Provider manifests are validated against the schema packaged with this crate.
Snapshots sort catalogs before computing their SHA-256 fingerprint, so the
same provider set has the same identity regardless of registration order.
