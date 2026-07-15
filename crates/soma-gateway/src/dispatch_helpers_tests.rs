use super::*;

#[test]
fn structured_errors_have_stable_shape() {
    let error = structured_error("gateway.test", "invalid_param", "validation", "fix params");
    let json = error.to_json();

    assert_eq!(json["isError"], true);
    assert_eq!(json["schema_version"], "soma.gateway.error.v1");
    assert_eq!(json["tool"], "gateway");
}
