use serde_json::json;
use soma_gateway::gateway::catalog::GatewayActionCatalog;

use super::{expand_env_templates, header_pairs, project_gateway_action_catalog, UpstreamTool};

#[test]
fn projects_every_standard_gateway_action_as_a_tool() {
    let actions = GatewayActionCatalog::standard();
    let catalog = project_gateway_action_catalog("gateway", "Gateway administration", &actions)
        .expect("`gateway` is a valid provider id");

    assert_eq!(catalog.tools.len(), actions.list().len());
    let reload = catalog
        .tools
        .iter()
        .find(|tool| tool.name == "gateway.reload")
        .expect("gateway.reload projected");
    assert!(reload.requires_admin);
    assert!(!reload.destructive);

    let remove = catalog
        .tools
        .iter()
        .find(|tool| tool.name == "gateway.remove")
        .expect("gateway.remove projected");
    assert!(remove.destructive);
}

#[test]
fn rejects_invalid_provider_id_instead_of_panicking() {
    let actions = GatewayActionCatalog::standard();
    let result = project_gateway_action_catalog("Not A Valid Id!", "Gateway", &actions);
    assert!(result.is_err());
}

// ── UpstreamTool::params() pin semantics ───────────────────────────────────

#[test]
fn static_args_win_over_caller_supplied_params_with_the_same_key() {
    let tool = UpstreamTool {
        name: "soma".to_owned(),
        static_args: json!({"action": "echo"}).as_object().unwrap().clone(),
    };

    // A caller attempting to override the manifest-pinned `action` must not
    // succeed — static_args are a pin, not a caller-overridable default.
    let merged = tool.params(json!({"action": "not_echo", "message": "hi"}));

    assert_eq!(merged.get("action").and_then(|v| v.as_str()), Some("echo"));
    assert_eq!(merged.get("message").and_then(|v| v.as_str()), Some("hi"));
}

#[test]
fn static_args_are_present_even_when_caller_supplies_no_params() {
    let tool = UpstreamTool {
        name: "soma".to_owned(),
        static_args: json!({"action": "echo"}).as_object().unwrap().clone(),
    };

    let merged = tool.params(json!({}));

    assert_eq!(merged.get("action").and_then(|v| v.as_str()), Some("echo"));
}

#[test]
fn params_tolerates_non_object_caller_params() {
    let tool = UpstreamTool {
        name: "soma".to_owned(),
        static_args: json!({"action": "echo"}).as_object().unwrap().clone(),
    };

    // A non-object `call.params` (e.g. null) must not panic; static_args
    // still apply.
    let merged = tool.params(serde_json::Value::Null);

    assert_eq!(merged.get("action").and_then(|v| v.as_str()), Some("echo"));
}

// ── expand_env_templates ────────────────────────────────────────────────────
//
// These tests read already-set environment variables rather than calling
// `std::env::set_var`/`remove_var` — this crate is `#![forbid(unsafe_code)]`
// and those setters require `unsafe` (process-wide, not thread-safe to
// mutate from a parallel test run) since Rust 1.82.

#[test]
fn expand_env_templates_substitutes_a_single_variable() {
    let path = std::env::var("PATH").expect("PATH is set in any test environment");
    let result = expand_env_templates("prefix-${PATH}-suffix");
    assert_eq!(result, Ok(format!("prefix-{path}-suffix")));
}

#[test]
fn expand_env_templates_substitutes_multiple_variables() {
    // CARGO_PKG_NAME, not HOME: Cargo sets this as a real process
    // environment variable for every `cargo test` run on every platform it
    // supports, unlike HOME — which Windows test runners don't set,
    // breaking this exact test there.
    let path = std::env::var("PATH").expect("PATH is set in any test environment");
    let pkg_name = std::env::var("CARGO_PKG_NAME")
        .expect("CARGO_PKG_NAME is set by cargo test on every platform");
    let result = expand_env_templates("${PATH}-${CARGO_PKG_NAME}");
    assert_eq!(result, Ok(format!("{path}-{pkg_name}")));
}

#[test]
fn expand_env_templates_passes_through_literal_text_with_no_placeholders() {
    assert_eq!(
        expand_env_templates("no placeholders here"),
        Ok("no placeholders here".to_owned())
    );
}

#[test]
fn expand_env_templates_rejects_missing_environment_variable() {
    // Extremely unlikely to be set in any test environment.
    let result = expand_env_templates("${SOMA_TEST_GATEWAY_DEFINITELY_UNSET_VAR}");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("missing environment variable"));
}

#[test]
fn expand_env_templates_rejects_unterminated_placeholder() {
    let result = expand_env_templates("Bearer ${UNTERMINATED");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("unterminated"));
}

#[test]
fn expand_env_templates_rejects_empty_variable_name() {
    let result = expand_env_templates("${}");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("requires a variable name"));
}

#[test]
fn header_pairs_surfaces_env_interpolation_failure() {
    let value = json!({"Authorization": "Bearer ${SOMA_TEST_GATEWAY_DEFINITELY_UNSET_VAR}"});
    let result = header_pairs(Some(&value));
    assert!(result.is_err());
}
