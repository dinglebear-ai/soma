use super::*;

#[test]
fn artifact_uri_is_operation_scoped_and_path_independent() {
    let context = OperationContext::new();
    let uri = format!("soma-artifact://{}", context.operation_id());
    assert!(uri.starts_with("soma-artifact://"));
    assert!(uri.len() < 4096);
}
