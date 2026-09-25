use super::*;
use soma_fleet::{HostId, TopologyRevision};
use soma_infra::BuildContextFingerprint;
use soma_ops::{
    ExecutionMetadata, MutationSendState, OperationId, OperationName, OperationResult,
    OperationStatus, RetryClass, TargetKind, TargetRef, Timestamp,
};
#[test]
fn build_context_evidence_uses_digest_not_raw_path() {
    let result = OperationResult::new(
        OperationId::new(),
        OperationName::new("docker.build").unwrap(),
        TargetRef::new(TargetKind::Image, "app:v1").unwrap(),
        OperationStatus::Succeeded,
        ExecutionMetadata::new(
            Timestamp::now(),
            Timestamp::now(),
            MutationSendState::Sent,
            RetryClass::Never,
        )
        .unwrap(),
    )
    .unwrap();
    let fp = BuildContextFingerprint {
        host: HostId::new("devhost").unwrap(),
        topology_revision: TopologyRevision::from_material(b"test"),
        path: "/private/path".into(),
        sha256: "a".repeat(64),
        file_count: 1,
        byte_count: 1,
    };
    let result = add_context_evidence(result, &fp).unwrap();
    assert!(!format!("{:?}", result.evidence()).contains("private/path"));
}
