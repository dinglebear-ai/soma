use soma_ops::{DiagnosticCode, OperationName};

use super::*;

#[test]
fn errors_preserve_stable_operation_and_diagnostic_identity() {
    let error = CompatibilityError::DiagnosticNotDeclared {
        operation: OperationName::new("host.exec").unwrap(),
        code: DiagnosticCode::new("docker.conflict").unwrap(),
    };
    assert_eq!(
        error.to_string(),
        "diagnostic docker.conflict is not declared by operation host.exec"
    );
}
