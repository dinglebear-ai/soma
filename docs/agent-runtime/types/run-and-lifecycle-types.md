---
title: "Agent Run and Lifecycle Types"
created: 2026-08-05
updated: 2026-08-05
doc_type: "types"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Run and Lifecycle Types

Proposed files: <code>agent_runtime/run.rs</code> and an application lifecycle module.

~~~rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::ids::{
    AgentId, ArtifactId, CanonicalRef, ContextGenerationId, LifecycleEventId,
    RunId, RuntimeInstanceId, ServiceId, Sha256Digest, StackId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentRunState {
    Created,
    Resolving,
    Resolved,
    Provisioning,
    Bootstrapping,
    Running,
    AwaitingApproval,
    Verifying,
    Finalizing,
    Stopping,
    Snapshotting,
    Succeeded,
    Failed,
    Cancelled,
    CleanupFailed,
}

impl AgentRunState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled | Self::CleanupFailed)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        use AgentRunState::*;
        matches!((self, next),
            (Created, Resolving)
            | (Resolving, Resolved | Failed | Cancelled)
            | (Resolved, Provisioning | Cancelled)
            | (Provisioning, Bootstrapping | Failed | Cancelled)
            | (Bootstrapping, Running | Failed | Cancelled)
            | (Running, AwaitingApproval | Verifying | Failed | Cancelled | Stopping)
            | (AwaitingApproval, Running | Failed | Cancelled)
            | (Verifying, Finalizing | Failed | Cancelled)
            | (Finalizing, Succeeded | Failed | CleanupFailed)
            | (Stopping, Finalizing | Cancelled | Failed)
            | (Snapshotting, Finalizing | CleanupFailed)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentRun {
    pub id: RunId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<RunId>,
    pub stack_id: StackId,
    pub service_id: ServiceId,
    pub agent_id: AgentId,
    pub state: AgentRunState,
    pub state_version: u64,
    pub attempt: u32,
    pub resolved_stack: CanonicalRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_generation_id: Option<ContextGenerationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_instance_id: Option<RuntimeInstanceId>,
    #[serde(default)]
    pub artifacts: Vec<ArtifactId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalOutcome>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LifecycleEvent {
    pub schema_version: String,
    pub id: LifecycleEventId,
    pub kind: String,
    pub event_time: String,
    pub ingestion_time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_generation_id: Option<ContextGenerationId>,
    pub source: EventSource,
    pub severity: EventSeverity,
    pub sensitivity: super::ids::Sensitivity,
    #[serde(default)]
    pub attributes: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub evidence: Vec<CanonicalRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SafeRuntimeError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RunManifest {
    pub schema_version: String,
    pub run: AgentRun,
    pub stack_digest: Sha256Digest,
    pub package_refs: Vec<CanonicalRef>,
    pub context_refs: Vec<CanonicalRef>,
    pub loadout_ref: CanonicalRef,
    pub runtime_ref: CanonicalRef,
    pub disclosure_summary: DisclosureSummary,
    pub output_refs: Vec<CanonicalRef>,
    pub verification: VerificationReport,
    pub digest: Sha256Digest,
}
~~~

Store-level lease, heartbeat, checkpoint, and event-sequence fields should follow Axon's unified job records rather than expanding the public domain aggregate prematurely.
