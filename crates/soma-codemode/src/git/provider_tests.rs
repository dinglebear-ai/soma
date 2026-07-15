use serde_json::json;

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
