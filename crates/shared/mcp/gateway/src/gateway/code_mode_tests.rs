use super::*;

#[test]
fn namespace_ids_are_namespace_colon_colon_tool() {
    let id = namespace_tool_id("axon", "search");

    assert_eq!(id, "axon::search");
    assert_eq!(
        parse_namespace_tool_id(&id).unwrap(),
        CodeModeToolId {
            namespace: "axon".to_owned(),
            tool: "search".to_owned()
        }
    );
}

#[test]
fn params_must_be_objects_and_error_kinds_are_preserved() {
    assert_eq!(
        ensure_object_params(&serde_json::json!("bad")),
        Err(CodeModeError::ParamsMustBeObject)
    );
    assert_eq!(
        preserve_gateway_error(GatewayCodeModeError {
            kind: "oauth_needs_reauth".to_owned(),
            message: "reauth".to_owned(),
        }),
        CodeModeError::Gateway {
            kind: "oauth_needs_reauth".to_owned(),
            message: "reauth".to_owned(),
        }
    );
}
