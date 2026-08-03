use super::*;
use crate::{
    BuildContextFingerprint, ComposeBuildArtifact, ComposeBuildReceipt, ComposeProjectRef,
    ImageSummary, InfraResult,
};
use async_trait::async_trait;
use soma_fleet::{HostEndpoint, HostId};
use soma_ops::{
    MutationSendState, NoopProgressSink, OperationId, OperationName, Timestamp, VerificationStatus,
};
use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::Mutex;

struct Contexts(Mutex<VecDeque<BuildContextFingerprint>>);
#[async_trait]
impl BuildContextInspector for Contexts {
    async fn fingerprint(
        &self,
        _: &HostRecord,
        _: &Path,
        _: Timestamp,
        _: &CancellationToken,
    ) -> InfraResult<BuildContextFingerprint> {
        Ok(self.0.lock().unwrap().pop_front().unwrap())
    }
}
struct Mutator;
#[async_trait]
impl ComposeBuildMutator for Mutator {
    async fn build_compose(
        &self,
        host: &HostRecord,
        request: &ComposeBuildRequest,
        _: &dyn MutationProgressReporter,
        _: &CancellationToken,
    ) -> MutationResult<ComposeBuildReceipt> {
        Ok(ComposeBuildReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().into(),
            service: request.service().map(str::to_owned),
            send_state: MutationSendState::Sent,
            stdout: String::new(),
            stderr: String::new(),
            output_truncated: false,
            progress_delivery_errors: Vec::new(),
        })
    }
}
struct Images(Mutex<VecDeque<Vec<ImageSummary>>>);
#[async_trait]
impl ImageReader for Images {
    async fn list_images(
        &self,
        _: &HostRecord,
        _: &ImageListOptions,
        _: &CancellationToken,
    ) -> InfraResult<Vec<ImageSummary>> {
        Ok(self.0.lock().unwrap().pop_front().unwrap())
    }
}
fn host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}
fn fp(path: &str, value: &str) -> BuildContextFingerprint {
    BuildContextFingerprint {
        host: host().id().clone(),
        topology_revision: host().revision().clone(),
        path: path.into(),
        sha256: value.repeat(64),
        file_count: 1,
        byte_count: 1,
    }
}
fn request(expected: BuildContextFingerprint) -> ComposeBuildRequest {
    ComposeBuildRequest::new(
        OperationId::new(),
        OperationName::new("compose.build").unwrap(),
        ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap(),
        Some("api".into()),
        vec![ComposeBuildArtifact {
            service: "api".into(),
            image: "soma-api:v1".into(),
            context: "/srv/api".into(),
            fingerprint: expected,
        }],
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap()
}
fn image() -> ImageSummary {
    ImageSummary {
        host: host().id().clone(),
        topology_revision: host().revision().clone(),
        id: "sha256:222".into(),
        repo_tags: vec!["soma-api:v1".into()],
        repo_digests: Vec::new(),
        created_unix_seconds: 0,
        size_bytes: 0,
        containers: 0,
        labels: BTreeMap::new(),
    }
}

#[tokio::test]
async fn compose_build_rejects_context_drift_before_send() {
    let error = ComposeBuildEngine
        .execute(
            ComposeBuildServices {
                contexts: &Contexts(Mutex::new(VecDeque::from([fp("/srv/api", "b")]))),
                mutator: &Mutator,
                images: &Images(Mutex::new(VecDeque::new())),
            },
            &host(),
            &request(fp("/srv/api", "a")),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.send_state(), MutationSendState::NotSent);
}
#[tokio::test]
async fn compose_build_verifies_every_output_image() {
    let expected = fp("/srv/api", "a");
    let images = Images(Mutex::new(VecDeque::from([Vec::new(), vec![image()]])));
    let outcome = ComposeBuildEngine
        .execute(
            ComposeBuildServices {
                contexts: &Contexts(Mutex::new(VecDeque::from([expected.clone()]))),
                mutator: &Mutator,
                images: &images,
            },
            &host(),
            &request(expected),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.verification_status, VerificationStatus::Verified);
    assert!(outcome.changed);
}
