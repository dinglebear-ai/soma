use super::*;
use crate::{BuildContextFingerprint, ImageBuildReceipt, ImageSummary, InfraResult};
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
struct Mutator(Mutex<usize>);
#[async_trait]
impl ImageBuildMutator for Mutator {
    async fn build_image(
        &self,
        host: &HostRecord,
        request: &ImageBuildRequest,
        _: &dyn MutationProgressReporter,
        _: &CancellationToken,
    ) -> MutationResult<ImageBuildReceipt> {
        *self.0.lock().unwrap() += 1;
        Ok(ImageBuildReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            tag: request.tag().into(),
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
fn fp(value: &str) -> BuildContextFingerprint {
    BuildContextFingerprint {
        host: host().id().clone(),
        topology_revision: host().revision().clone(),
        path: "/srv/app".into(),
        sha256: value.repeat(64),
        file_count: 1,
        byte_count: 1,
    }
}
fn request(expected: BuildContextFingerprint) -> ImageBuildRequest {
    ImageBuildRequest::new(
        OperationId::new(),
        OperationName::new("docker.build").unwrap(),
        "/srv/app".into(),
        None,
        "app:v1",
        false,
        expected,
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap()
}
fn image() -> ImageSummary {
    ImageSummary {
        host: host().id().clone(),
        topology_revision: host().revision().clone(),
        id: "sha256:111".into(),
        repo_tags: vec!["app:v1".into()],
        repo_digests: Vec::new(),
        created_unix_seconds: 0,
        size_bytes: 0,
        containers: 0,
        labels: BTreeMap::new(),
    }
}

#[tokio::test]
async fn context_drift_rejects_before_build_send() {
    let mutator = Mutator(Mutex::new(0));
    let images = Images(Mutex::new(VecDeque::new()));
    let error = ImageBuildEngine
        .execute(
            ImageBuildServices {
                contexts: &Contexts(Mutex::new(VecDeque::from([fp("b")]))),
                mutator: &mutator,
                images: &images,
            },
            &host(),
            &request(fp("a")),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.send_state(), MutationSendState::NotSent);
    assert_eq!(*mutator.0.lock().unwrap(), 0);
}
#[tokio::test]
async fn completed_build_requires_local_output_identity() {
    let expected = fp("a");
    let images = Images(Mutex::new(VecDeque::from([Vec::new(), Vec::new()])));
    let outcome = ImageBuildEngine
        .execute(
            ImageBuildServices {
                contexts: &Contexts(Mutex::new(VecDeque::from([expected.clone()]))),
                mutator: &Mutator(Mutex::new(0)),
                images: &images,
            },
            &host(),
            &request(expected),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.verification_status, VerificationStatus::Failed);
}
#[tokio::test]
async fn completed_build_verifies_output_tag() {
    let expected = fp("a");
    let images = Images(Mutex::new(VecDeque::from([Vec::new(), vec![image()]])));
    let outcome = ImageBuildEngine
        .execute(
            ImageBuildServices {
                contexts: &Contexts(Mutex::new(VecDeque::from([expected.clone()]))),
                mutator: &Mutator(Mutex::new(0)),
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
