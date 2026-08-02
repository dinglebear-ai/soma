use serde_json::json;
use soma_ops::{DiagnosticCode, OperationName};

use super::*;

fn operation(name: &str) -> OperationName {
    OperationName::new(name).unwrap()
}

#[test]
fn mutation_and_command_results_project_deterministically() {
    let catalog = SynapseCatalog::embedded();
    let mutation = catalog
        .project_result(
            &operation("container.restart"),
            &json!({"changed":true,"action":"restart","summary":"container restarted"}),
            LegacyPresentation::Markdown,
        )
        .unwrap();
    assert_eq!(
        mutation,
        LegacyProjectedResult::Markdown("container restarted".into())
    );

    let command = catalog
        .project_result(
            &operation("host.exec"),
            &json!({
                "exit_code":0,
                "stdout":"dookie",
                "timed_out":false,
                "truncated":false
            }),
            LegacyPresentation::Markdown,
        )
        .unwrap();
    let LegacyProjectedResult::Markdown(command) = command else {
        panic!("expected markdown")
    };
    assert!(command.contains("Exit code:** 0"));
    assert!(command.contains("dookie"));
}

#[test]
fn text_artifacts_and_json_payloads_are_preserved() {
    let catalog = SynapseCatalog::embedded();
    let artifact = catalog
        .project_result(
            &operation("container.logs"),
            &json!({
                "content_artifact": {
                    "uri":"artifact://logs/1",
                    "media_type":"text/plain",
                    "bytes":500000,
                    "protected":true
                },
                "bytes":500000,
                "truncated":true,
                "encoding":"utf-8"
            }),
            LegacyPresentation::Markdown,
        )
        .unwrap();
    assert_eq!(
        artifact,
        LegacyProjectedResult::Markdown("Protected artifact: `artifact://logs/1`".into())
    );

    let payload = json!({"status":"running"});
    assert_eq!(
        catalog
            .project_result(
                &operation("host.status"),
                &payload,
                LegacyPresentation::Json
            )
            .unwrap(),
        LegacyProjectedResult::Json(payload)
    );
}

#[test]
fn invalid_results_are_rejected_before_projection() {
    assert!(matches!(
        SynapseCatalog::embedded().project_result(
            &operation("container.restart"),
            &json!({"changed":true,"action":"restart","summary":"ok","extra":1}),
            LegacyPresentation::Json
        ),
        Err(CompatibilityError::SchemaValidation { .. })
    ));
}

#[test]
fn diagnostics_are_enforced_per_operation() {
    let catalog = SynapseCatalog::embedded();
    let timeout = DiagnosticCode::new("operation.timeout").unwrap();
    let projection = catalog
        .project_diagnostic(&operation("host.exec"), &timeout)
        .unwrap();
    assert_eq!(projection.cli_exit_code(), 7);
    assert_eq!(projection.http_status(), 504);
    assert_eq!(projection.mcp_error_code(), -32008);

    let docker = DiagnosticCode::new("docker.conflict").unwrap();
    assert!(matches!(
        catalog.project_diagnostic(&operation("host.exec"), &docker),
        Err(CompatibilityError::DiagnosticNotDeclared { .. })
    ));
}
