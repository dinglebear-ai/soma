# soma-ops

Transport-neutral contracts for infrastructure operations.

`soma-ops` is a standalone shared crate. It contains no Soma or Synapse
product policy and has no dependency on MCP, HTTP, Docker, Incus, SSH, a
runtime, or a database.

It provides:

- stable operation, event, correlation, and authorization identities;
- validated dotted operation names and typed targets;
- operation catalog metadata and typed operation definitions;
- request context, deadlines, idempotency, and opaque authorization evidence;
- deterministic plan fingerprints;
- bounded progress events;
- results that distinguish transport success from verification;
- lifecycle events suitable for Cortex ingestion;
- optional JSON Schema derivation through the `schema` feature.

## Example

```rust
use serde::{Deserialize, Serialize};
use soma_ops::{
    AccessClass, OperationContext, OperationDefinition, OperationName,
    OperationRequest, OperationSpec, TargetKind, TargetRef, Timestamp,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct InspectHost {
    host: String,
}

struct InspectHostOperation;

impl OperationDefinition for InspectHostOperation {
    type Parameters = InspectHost;
    type Output = serde_json::Value;

    fn spec() -> OperationSpec {
        OperationSpec::new(
            OperationName::new("host.inspect").unwrap(),
            TargetKind::Host,
            AccessClass::Read,
        )
    }

    fn target(parameters: &Self::Parameters) -> Result<TargetRef, soma_ops::TargetRefError> {
        TargetRef::new(TargetKind::Host, parameters.host.clone())
    }
}

let parameters = InspectHost { host: "dookie".into() };
let request = OperationRequest::new::<InspectHostOperation>(
    OperationContext::new(),
    parameters,
)?;
request.validate_against(&InspectHostOperation::spec(), Timestamp::now(), None)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Boundary

The crate answers: **what is an operation, how is it authorized, planned,
reported, verified, and recorded?**

It does not answer: **which user may approve it, which hosts exist, or how a
Docker/Incus/SSH operation is executed?** Those belong to product policy,
`soma-fleet`, and `soma-infra`.
