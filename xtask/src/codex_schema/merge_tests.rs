use super::*;
use serde_json::json;

/// The exact raw shape `codex app-server generate-json-schema` emits for
/// `McpServerElicitationRequestParams` (captured pre-fix, straight from the
/// master bundle) - a top-level object (`serverName`/`threadId`/`turnId`)
/// with a sibling `oneOf`, one branch of which has a wildcard (`true`)
/// sub-schema. This is the exact shape that panics typify 0.7.0's
/// schema-merge logic; see the crate README's "How the typed protocol layer
/// is built" section.
const RAW_MCP_ELICITATION_PARAMS: &str = include_str!("testdata/raw_mcp_elicitation_params.json");

#[test]
fn rewrite_v2_refs_rewrites_nested_refs_recursively() {
    let input = json!({
        "type": "object",
        "properties": {
            "a": {"$ref": "#/definitions/v2/Foo"},
            "b": {"anyOf": [{"$ref": "#/definitions/v2/Bar"}, {"type": "null"}]},
            "c": {"$ref": "#/definitions/Unrelated"}
        }
    });
    let out = rewrite_v2_refs(&input);
    assert_eq!(out["properties"]["a"]["$ref"], "#/definitions/Foo");
    assert_eq!(
        out["properties"]["b"]["anyOf"][0]["$ref"],
        "#/definitions/Bar"
    );
    // Non-v2-prefixed refs are left untouched.
    assert_eq!(out["properties"]["c"]["$ref"], "#/definitions/Unrelated");
}

#[test]
fn flatten_base_plus_oneof_matches_the_real_mcp_elicitation_shape() {
    let raw: Value = serde_json::from_str(RAW_MCP_ELICITATION_PARAMS).unwrap();
    let flattened = flatten_base_plus_oneof(&raw).unwrap();

    assert_eq!(flattened["title"], "McpServerElicitationRequestParams");
    let branches = flattened["oneOf"].as_array().unwrap();
    assert_eq!(branches.len(), 3);

    let form_branch = &branches[0];
    assert_eq!(form_branch["type"], "object");
    // base props (serverName, threadId, turnId) + branch props (present).
    assert!(form_branch["properties"]["serverName"].is_object());
    assert!(form_branch["properties"]["mode"].is_object());
    assert_eq!(
        form_branch["required"],
        json!([
            "message",
            "mode",
            "requestedSchema",
            "serverName",
            "threadId"
        ])
    );

    // Branch 2 ("openai/form") is the one with the wildcard `true`
    // sub-schema that panics typify pre-flatten.
    let wildcard_branch = &branches[1];
    assert_eq!(
        wildcard_branch["properties"]["requestedSchema"],
        json!(true)
    );

    let url_branch = &branches[2];
    assert_eq!(
        url_branch["required"],
        json!([
            "elicitationId",
            "message",
            "mode",
            "serverName",
            "threadId",
            "url"
        ])
    );
}

#[test]
fn flatten_base_plus_oneof_is_a_no_op_without_the_base_plus_oneof_shape() {
    let plain = json!({"type": "string"});
    assert_eq!(flatten_base_plus_oneof(&plain).unwrap(), plain);

    let oneof_only = json!({"oneOf": [{"type": "string"}]});
    assert_eq!(flatten_base_plus_oneof(&oneof_only).unwrap(), oneof_only);

    let properties_only = json!({"properties": {"a": {"type": "string"}}});
    assert_eq!(
        flatten_base_plus_oneof(&properties_only).unwrap(),
        properties_only
    );
}

#[test]
fn flatten_base_plus_oneof_bails_on_a_ref_branch() {
    // A `{"$ref": ...}` branch is still a JSON *object* (so it passes the
    // "is this an object at all" check), it's just missing "type": "object" -
    // falls through to the second (type) check rather than the first.
    let schema = json!({
        "properties": {"base": {"type": "string"}},
        "oneOf": [{"$ref": "#/definitions/SomeNamedBranch"}]
    });
    let err = flatten_base_plus_oneof(&schema).unwrap_err();
    assert!(err.to_string().contains("is not \"type\": \"object\""));
}

#[test]
fn flatten_base_plus_oneof_bails_on_a_non_object_branch() {
    let schema = json!({
        "properties": {"base": {"type": "string"}},
        "oneOf": ["not even an object"]
    });
    let err = flatten_base_plus_oneof(&schema).unwrap_err();
    assert!(err.to_string().contains("not an inline object schema"));
}

#[test]
fn flatten_base_plus_oneof_bails_on_a_branch_with_no_properties() {
    let schema = json!({
        "properties": {"base": {"type": "string"}},
        "oneOf": [{"type": "object"}]
    });
    let err = flatten_base_plus_oneof(&schema).unwrap_err();
    assert!(err.to_string().contains("has no \"properties\" key"));
}

#[test]
fn build_combined_v2_wins_collisions_and_keeps_master_only_keys() {
    // "RequestId" is on EXPECTED_DIVERGENT_COLLISIONS, so the two bundles are
    // allowed to disagree on its content here without check_collision_compatibility
    // rejecting the merge (see the dedicated bail/allow tests below for that
    // check's own behavior).
    //
    // Deliberately does not assert on `defs.keys()`' iteration order: without
    // the (removed - see xtask/Cargo.toml) `preserve_order` feature,
    // `serde_json::Map` is a `BTreeMap` and always iterates sorted regardless
    // of insertion sequence, so no order guarantee is meaningful to test here
    // - only which keys are present and which value won each collision.
    let master = json!({
        "definitions": {
            "A": {"marker": "master"},
            "RequestId": {"marker": "master"},
            "v2": {"C": {"marker": "should-be-ignored"}}
        }
    });
    let v2 = json!({
        "definitions": {
            "RequestId": {"marker": "v2"},
            "C": {"marker": "v2"}
        }
    });
    let combined = build_combined(&master, &v2).unwrap();
    let defs = combined["definitions"].as_object().unwrap();
    let mut keys: Vec<&str> = defs.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["A", "C", "RequestId"]);
    assert_eq!(defs["A"]["marker"], "master");
    assert_eq!(defs["RequestId"]["marker"], "v2"); // v2 wins the value
    assert_eq!(defs["C"]["marker"], "v2"); // genuinely new key from v2, included
}

#[test]
fn build_combined_bails_on_an_unexpected_divergent_collision() {
    let master = json!({"definitions": {"SomeType": {"marker": "master"}}});
    let v2 = json!({"definitions": {"SomeType": {"marker": "v2"}}});
    let err = build_combined(&master, &v2).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("SomeType"));
    assert!(msg.contains("EXPECTED_DIVERGENT_COLLISIONS"));
}

#[test]
fn build_combined_allows_identical_content_collisions() {
    // Same name, byte-identical content in both bundles - not a real
    // divergence, must merge without complaint even though it's not on the
    // explicit allowlist.
    let master = json!({"definitions": {"Shared": {"marker": "same"}}});
    let v2 = json!({"definitions": {"Shared": {"marker": "same"}}});
    let combined = build_combined(&master, &v2).unwrap();
    assert_eq!(combined["definitions"]["Shared"]["marker"], "same");
}

#[test]
fn build_combined_rewrites_v2_refs_in_master_only_definitions() {
    let master = json!({
        "definitions": {
            "Envelope": {"$ref": "#/definitions/v2/Inner"}
        }
    });
    let v2 = json!({"definitions": {"Inner": {"type": "string"}}});
    let combined = build_combined(&master, &v2).unwrap();
    assert_eq!(
        combined["definitions"]["Envelope"]["$ref"],
        "#/definitions/Inner"
    );
}

#[test]
fn build_combined_applies_the_elicitation_flatten_workaround() {
    let raw: Value = serde_json::from_str(RAW_MCP_ELICITATION_PARAMS).unwrap();
    let master = json!({"definitions": {"McpServerElicitationRequestParams": raw}});
    let v2 = json!({"definitions": {}});
    let combined = build_combined(&master, &v2).unwrap();
    let out = &combined["definitions"]["McpServerElicitationRequestParams"];
    // Flattened shape has no top-level "properties" (only "title"/"oneOf").
    assert!(out.get("properties").is_none());
    assert!(out.get("oneOf").is_some());
}

#[test]
fn params_type_for_recognizes_all_four_allowed_shapes() {
    let union = json!({
        "oneOf": [
            {"properties": {"method": {"enum": ["plain_ref"]}, "params": {"$ref": "#/definitions/Foo"}}},
            {"properties": {"method": {"enum": ["nullable_ref"]}, "params": {"anyOf": [{"$ref": "#/definitions/Bar"}, {"type": "null"}]}}},
            {"properties": {"method": {"enum": ["explicit_null"]}, "params": {"type": "null"}}},
            {"properties": {"method": {"enum": ["no_params_key"]}}}
        ]
    });

    let p = params_type_for("plain_ref", &union).unwrap();
    assert_eq!(p.type_name.as_deref(), Some("Foo"));
    assert!(!p.optional);

    let p = params_type_for("nullable_ref", &union).unwrap();
    assert_eq!(p.type_name.as_deref(), Some("Bar"));
    assert!(p.optional);

    let p = params_type_for("explicit_null", &union).unwrap();
    assert_eq!(p.type_name, None);
    assert!(!p.optional);

    let p = params_type_for("no_params_key", &union).unwrap();
    assert_eq!(p.type_name, None);
    assert!(!p.optional);
}

#[test]
fn params_type_for_hard_fails_on_unrecognized_shapes() {
    let union = json!({
        "oneOf": [
            {"properties": {"method": {"enum": ["weird"]}, "params": {"type": "object", "properties": {}}}}
        ]
    });
    let err = params_type_for("weird", &union).unwrap_err();
    assert!(err
        .to_string()
        .contains("unrecognized 'params' schema shape"));
}

#[test]
fn params_type_for_hard_fails_on_three_way_anyof() {
    let union = json!({
        "oneOf": [
            {"properties": {"method": {"enum": ["weird"]}, "params": {"anyOf": [
                {"$ref": "#/definitions/A"}, {"$ref": "#/definitions/B"}, {"type": "null"}
            ]}}}
        ]
    });
    let err = params_type_for("weird", &union).unwrap_err();
    assert!(err
        .to_string()
        .contains("unrecognized 'params' schema shape"));
}

#[test]
fn params_type_for_hard_fails_when_method_missing() {
    let union = json!({"oneOf": []});
    let err = params_type_for("missing", &union).unwrap_err();
    assert!(err
        .to_string()
        .contains("not found in the given union's oneOf branches"));
}

#[test]
fn methods_of_extracts_method_enum_values_in_order() {
    let union = json!({
        "oneOf": [
            {"properties": {"method": {"enum": ["a"]}}},
            {"properties": {"method": {"enum": ["b"]}}}
        ]
    });
    assert_eq!(methods_of(&union).unwrap(), vec!["a", "b"]);
}

#[test]
fn build_methods_manifest_end_to_end_on_a_minimal_schema() {
    let mut defs = Map::new();
    defs.insert(
        "ClientRequest".to_string(),
        json!({"oneOf": [
            {"properties": {"method": {"enum": ["thread/start"]}, "params": {"$ref": "#/definitions/ThreadStartParams"}}}
        ]}),
    );
    defs.insert("ServerRequest".to_string(), json!({"oneOf": []}));
    defs.insert("ServerNotification".to_string(), json!({"oneOf": [
        {"properties": {"method": {"enum": ["error"]}, "params": {"$ref": "#/definitions/ErrorNotification"}}}
    ]}));
    defs.insert(
        "ClientNotification".to_string(),
        json!({"oneOf": [
            {"properties": {"method": {"enum": ["initialized"]}}}
        ]}),
    );
    defs.insert("ThreadStartResponse".to_string(), json!({}));

    let manifest = build_methods_manifest(&defs).unwrap();
    assert_eq!(manifest.client_requests.len(), 1);
    let entry = &manifest.client_requests[0];
    assert_eq!(entry.method, "thread/start");
    assert_eq!(entry.variant_name, "ThreadStart");
    assert_eq!(entry.fn_name, "thread_start");
    assert_eq!(entry.params_type.as_deref(), Some("ThreadStartParams"));
    assert_eq!(entry.response_type.as_deref(), Some("ThreadStartResponse"));

    assert_eq!(manifest.server_notifications.len(), 1);
    assert_eq!(manifest.server_notifications[0].fn_name, "error");

    assert_eq!(manifest.client_notifications.len(), 1);
    assert_eq!(manifest.client_notifications[0].fn_name, "initialized");
    assert_eq!(manifest.client_notifications[0].params_type, None);
}

#[test]
fn notification_entries_serialize_without_a_response_type_field() {
    let entry = NotificationEntry {
        method: "initialized".to_string(),
        variant_name: "Initialized".to_string(),
        fn_name: "initialized".to_string(),
        params_type: None,
        params_optional: false,
    };
    // Serialized directly to a string (as `regen.rs` does - see its comment
    // on avoiding a `serde_json::Value` intermediate), not via
    // `serde_json::to_value(&entry).as_object().keys()`: a struct's field
    // order is only guaranteed through the direct string path. Routing
    // through `Value` first loses it, since `Value::Object`'s `Map`
    // re-sorts keys without the (deliberately not enabled - see
    // xtask/Cargo.toml) `preserve_order` feature.
    let text = serde_json::to_string(&entry).unwrap();
    assert!(!text.contains("response_type"));
    let method_pos = text.find("\"method\"").unwrap();
    let variant_pos = text.find("\"variant_name\"").unwrap();
    let fn_pos = text.find("\"fn_name\"").unwrap();
    let params_type_pos = text.find("\"params_type\"").unwrap();
    let params_optional_pos = text.find("\"params_optional\"").unwrap();
    assert!(method_pos < variant_pos);
    assert!(variant_pos < fn_pos);
    assert!(fn_pos < params_type_pos);
    assert!(params_type_pos < params_optional_pos);
}

#[test]
fn request_entries_serialize_with_response_type_last() {
    let entry = RequestEntry {
        method: "thread/start".to_string(),
        variant_name: "ThreadStart".to_string(),
        fn_name: "thread_start".to_string(),
        params_type: Some("ThreadStartParams".to_string()),
        params_optional: false,
        response_type: Some("ThreadStartResponse".to_string()),
    };
    let text = serde_json::to_string(&entry).unwrap();
    let method_pos = text.find("\"method\"").unwrap();
    let variant_pos = text.find("\"variant_name\"").unwrap();
    let fn_pos = text.find("\"fn_name\"").unwrap();
    let params_type_pos = text.find("\"params_type\"").unwrap();
    let params_optional_pos = text.find("\"params_optional\"").unwrap();
    let response_type_pos = text.find("\"response_type\"").unwrap();
    assert!(method_pos < variant_pos);
    assert!(variant_pos < fn_pos);
    assert!(fn_pos < params_type_pos);
    assert!(params_type_pos < params_optional_pos);
    assert!(params_optional_pos < response_type_pos);
}
