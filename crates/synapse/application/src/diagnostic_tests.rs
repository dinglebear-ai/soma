use soma_ops::DiagnosticCode;

use crate::SynapseCatalog;

#[test]
fn embedded_projection_exposes_typed_surface_values() {
    let projection = SynapseCatalog::embedded()
        .diagnostic_projection(&DiagnosticCode::new("request.invalid").unwrap())
        .unwrap();
    assert_eq!(projection.category(), "input");
    assert_eq!(projection.cli_exit_code(), 2);
    assert_eq!(projection.http_status(), 400);
    assert_eq!(projection.mcp_error_code(), -32602);
    assert!(projection.terminal());
}
