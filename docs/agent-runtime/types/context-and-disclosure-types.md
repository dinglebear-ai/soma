---
title: "Context and Disclosure Types"
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

# Context and Disclosure Types

Proposed files: <code>agent_runtime/context.rs</code> and <code>agent_runtime/disclosure.rs</code>.

~~~rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::ids::{
    CanonicalRef, ContextGenerationId, ContextId, DisclosureDecisionId,
    DisclosureRequestId, EvidenceClass, RunId, Sensitivity, Sha256Digest,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContextManifest {
    pub api_version: String,
    pub kind: String,
    pub metadata: ContextMetadata,
    pub spec: ContextSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContextSpec {
    #[serde(default)]
    pub roots: Vec<ContextRoot>,
    #[serde(default)]
    pub sources: BTreeMap<String, ContextSourceSpec>,
    #[serde(default)]
    pub views: BTreeMap<String, ContextView>,
    #[serde(default)]
    pub policies: ContextPolicy,
    #[serde(default)]
    pub budgets: ContextBudgets,
    #[serde(default)]
    pub materializations: Vec<MaterializationKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CompileContextRequest {
    pub task: String,
    pub manifest: CanonicalRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    #[serde(default)]
    pub parameters: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    pub snapshot_mode: ContextSnapshotMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContextSnapshotMode { Reference, Portable, Forensic }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CompiledContext {
    pub id: ContextId,
    pub generation_id: ContextGenerationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_generation_id: Option<ContextGenerationId>,
    pub manifest: CanonicalRef,
    pub request_digest: Sha256Digest,
    pub plan_digest: Sha256Digest,
    pub graph_revision: String,
    pub task: String,
    pub roots: Vec<ContextRoot>,
    pub time_window: TimeWindow,
    pub counts: ContextCounts,
    pub sources: Vec<ContextSourceReport>,
    pub items: Vec<ContextItem>,
    pub conflicts: Vec<ContextConflict>,
    pub budget: BudgetReport,
    pub materializations: Vec<MaterializationReceipt>,
    pub created_at: String,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContextItem {
    pub id: String,
    pub canonical_ref: CanonicalRef,
    pub kind: String,
    pub evidence_class: EvidenceClass,
    pub sensitivity: Sensitivity,
    pub freshness: Freshness,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    #[serde(default)]
    pub entity_ids: Vec<String>,
    #[serde(default)]
    pub attributes: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisclosureLevel { Bootstrap, Orientation, Focused, Evidence, Raw, Expanded }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DisclosureRequest {
    pub id: DisclosureRequestId,
    pub run_id: RunId,
    pub context_generation_id: ContextGenerationId,
    pub requested_level: DisclosureLevel,
    pub purpose: String,
    #[serde(default)]
    pub selectors: Vec<DisclosureSelector>,
    pub representation: DisclosureRepresentation,
    #[serde(default)]
    pub budget: DisclosureBudget,
    #[serde(default)]
    pub raw_evidence: bool,
    pub requested_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisclosureDecisionStatus { Allowed, Narrowed, Denied, ApprovalRequired }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DisclosureDecision {
    pub id: DisclosureDecisionId,
    pub request_id: DisclosureRequestId,
    pub status: DisclosureDecisionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_level: Option<DisclosureLevel>,
    #[serde(default)]
    pub reason_codes: Vec<String>,
    #[serde(default)]
    pub selected_item_ids: Vec<String>,
    #[serde(default)]
    pub omitted_item_ids: Vec<String>,
    pub decided_at: String,
}
~~~

The implementation should keep large item bodies out of the aggregate and use canonical hydration or artifacts.
