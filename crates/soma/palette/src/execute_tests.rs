use serde_json::json;
use soma_domain::{AuthorizationMode, Confirmation};
use soma_provider_core::{ProviderId, ProviderManifest, ToolSpec};

use super::{confirmation_for, execute_launcher, ExecuteOutcome};
use crate::{dto::LauncherExecuteRequest, state::PaletteState};

#[test]
fn confirmed_flag_maps_to_confirmed() {
    assert_eq!(confirmation_for(true), Confirmation::Confirmed);
}

#[test]
fn unconfirmed_flag_maps_to_missing() {
    assert_eq!(confirmation_for(false), Confirmation::Missing);
}

/// Build a `PaletteState` backed by a single fixture provider exposing one
/// tool, so `execute_launcher`'s full resolve-then-dispatch path (find in
/// catalog, apply destructive confirmation, call `SomaApplication`) can be
/// exercised without a live HTTP server.
fn fixture_state(tool: ToolSpec, output: serde_json::Value) -> PaletteState {
    let mut manifest = ProviderManifest::new(
        ProviderId::new("fixture").expect("valid provider id"),
        "fixture",
        "0.1.0",
    );
    manifest.tools = vec![tool];
    let application = soma_test_support::application_with_provider(manifest, output);
    PaletteState::new(application, AuthorizationMode::LoopbackDev)
}

fn request(id: &str, confirm_destructive: bool) -> LauncherExecuteRequest {
    LauncherExecuteRequest {
        id: id.to_string(),
        params: json!({}),
        confirm_destructive,
    }
}

#[tokio::test]
async fn unknown_id_returns_not_found() {
    let tool = ToolSpec::new("ping", "Ping", json!({"type": "object"}));
    let state = fixture_state(tool, json!({"pong": true}));
    let context = state.execution_context(None, &[]);

    let outcome = execute_launcher(&state, request("does_not_exist", false), context).await;

    assert!(matches!(outcome, ExecuteOutcome::NotFound));
}

#[tokio::test]
async fn known_id_dispatches_and_returns_output() {
    let tool = ToolSpec::new("ping", "Ping", json!({"type": "object"}));
    let state = fixture_state(tool, json!({"pong": true}));
    let context = state.execution_context(None, &[]);

    let outcome = execute_launcher(&state, request("ping", false), context).await;

    match outcome {
        ExecuteOutcome::Ok(response) => {
            assert_eq!(response.output, json!({"pong": true}));
            assert!(response.request_id.starts_with("palette-"));
        }
        _ => panic!("expected ExecuteOutcome::Ok"),
    }
}

#[tokio::test]
async fn destructive_tool_without_confirmation_fails() {
    let mut tool = ToolSpec::new("delete_all", "Delete everything", json!({"type": "object"}));
    tool.destructive = true;
    let state = fixture_state(tool, json!({"ok": true}));
    let context = state.execution_context(None, &[]);

    let outcome = execute_launcher(&state, request("delete_all", false), context).await;

    match outcome {
        ExecuteOutcome::Failed(error) => assert_eq!(error.code, "confirmation_required"),
        _ => panic!("expected ExecuteOutcome::Failed(confirmation_required)"),
    }
}

#[tokio::test]
async fn destructive_tool_with_confirmation_succeeds() {
    let mut tool = ToolSpec::new("delete_all", "Delete everything", json!({"type": "object"}));
    tool.destructive = true;
    let state = fixture_state(tool, json!({"ok": true}));
    let context = state.execution_context(None, &[]);

    let outcome = execute_launcher(&state, request("delete_all", true), context).await;

    assert!(matches!(outcome, ExecuteOutcome::Ok(_)));
}
