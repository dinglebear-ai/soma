use serde_json::json;
use std::process::Command;

use super::provider::GitProvider;

#[tokio::test]
async fn git_provider_validates_ref_before_command_shape() {
    let provider = GitProvider::new(".");
    let error = provider
        .dispatch("show_ref", json!({"ref": "-bad"}))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), "invalid_param");
}

#[tokio::test]
async fn git_provider_reports_non_repo_status_failure() {
    let temp = tempfile::tempdir().unwrap();
    let provider = GitProvider::new(temp.path());

    let error = provider.dispatch("status", json!({})).await.unwrap_err();

    assert_eq!(error.kind(), "upstream_error");
}

#[tokio::test]
async fn git_provider_resolves_existing_ref_and_rejects_missing_ref() {
    let temp = tempfile::tempdir().unwrap();
    git(temp.path(), ["init"]);
    git(temp.path(), ["config", "user.email", "soma@example.com"]);
    git(temp.path(), ["config", "user.name", "Soma"]);
    std::fs::write(temp.path().join("README.md"), "hello").unwrap();
    git(temp.path(), ["add", "README.md"]);
    git(temp.path(), ["commit", "-m", "init"]);
    let provider = GitProvider::new(temp.path());

    let head = provider
        .dispatch("show_ref", json!({"ref": "HEAD"}))
        .await
        .unwrap();
    let missing = provider
        .dispatch("show_ref", json!({"ref": "refs/heads/missing"}))
        .await
        .unwrap_err();

    assert_eq!(head["ref"], "HEAD");
    assert!(head["oid"].as_str().unwrap().len() >= 7);
    assert_eq!(missing.kind(), "upstream_error");
}

fn git<const N: usize>(cwd: &std::path::Path, args: [&str; N]) {
    let status = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success());
}
