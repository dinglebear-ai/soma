use serde::{Deserialize, Serialize};
use soma_ops::{
    AccessClass, OperationContext, OperationDefinition, OperationName, OperationRequest,
    OperationSpec, TargetKind, TargetRef, TargetRefError, Timestamp,
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
            OperationName::new("host.inspect").expect("static operation name is valid"),
            TargetKind::Host,
            AccessClass::Read,
        )
    }

    fn target(parameters: &Self::Parameters) -> Result<TargetRef, TargetRefError> {
        TargetRef::new(TargetKind::Host, parameters.host.clone())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = OperationRequest::new::<InspectHostOperation>(
        OperationContext::new().with_deadline(Timestamp::from_unix_millis(10_000)),
        InspectHost {
            host: "standalone-host".into(),
        },
    )?;
    request.validate_against(
        &InspectHostOperation::spec(),
        Timestamp::from_unix_millis(1),
        None,
    )?;
    println!("{} {}", request.operation(), request.target().id());
    Ok(())
}
