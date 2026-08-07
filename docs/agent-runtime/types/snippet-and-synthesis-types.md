---
title: "Snippet and Synthesis Types"
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

# Snippet and Synthesis Types

Proposed files: <code>agent_runtime/snippet.rs</code> and <code>agent_runtime/synthesis.rs</code>.

~~~rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::capability::MutationClass;
use super::ids::{
    ArtifactId, CanonicalRef, ClaimId, ContextGenerationId, EvidenceClass,
    ResearchQuestionId, RunId, Sha256Digest, SnippetExecutionId, SnippetId,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SnippetDefinition {
    pub id: SnippetId,
    pub name: String,
    pub version: String,
    pub description: String,
    pub source: CanonicalRef,
    pub digest: Sha256Digest,
    pub risk_class: MutationClass,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub inputs: BTreeMap<String, SnippetInputSpec>,
    #[serde(default)]
    pub skills: Vec<PrimitiveRequirement>,
    #[serde(default)]
    pub context_domains: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub snippets: Vec<PrimitiveRequirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<CanonicalRef>,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SnippetInputSpec {
    pub input_type: SnippetInputType,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnippetInputType { String, Number, Boolean, Json, Duration, Uri, EntityId, ContextId }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SnippetExecutionResult {
    pub id: SnippetExecutionId,
    pub run_id: RunId,
    pub snippet: CanonicalRef,
    pub context_generation_id: ContextGenerationId,
    pub status: SnippetExecutionStatus,
    pub value: serde_json::Value,
    #[serde(default)]
    pub evidence: Vec<CanonicalRef>,
    #[serde(default)]
    pub artifacts: Vec<ArtifactId>,
    #[serde(default)]
    pub calls: Vec<ToolCallSummary>,
    pub budget: BudgetReport,
    pub started_at: String,
    pub finished_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnippetExecutionStatus { Completed, Failed, Cancelled, BudgetExhausted, ApprovalRequired }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SynthesisRequest {
    pub run_id: RunId,
    pub context_generation_id: ContextGenerationId,
    pub task: String,
    pub output_schema: CanonicalRef,
    #[serde(default)]
    pub allowed_snippets: Vec<String>,
    #[serde(default)]
    pub research: ResearchPolicy,
    #[serde(default)]
    pub budgets: SynthesisBudgets,
    #[serde(default)]
    pub stopping: StoppingPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SynthesisResult {
    pub id: String,
    pub run_id: RunId,
    pub context_generations: Vec<ContextGenerationId>,
    pub status: SynthesisStatus,
    pub summary: String,
    #[serde(default)]
    pub claims: Vec<Claim>,
    #[serde(default)]
    pub findings: Vec<Finding>,
    #[serde(default)]
    pub conflicts: Vec<Conflict>,
    #[serde(default)]
    pub rejected_hypotheses: Vec<Hypothesis>,
    #[serde(default)]
    pub open_questions: Vec<ResearchQuestion>,
    #[serde(default)]
    pub recommended_actions: Vec<RecommendedAction>,
    pub budget: BudgetReport,
    pub verification: VerificationReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Claim {
    pub id: ClaimId,
    pub text: String,
    pub status: EvidenceClass,
    pub confidence: f32,
    #[serde(default)]
    pub support: Vec<CanonicalRef>,
    #[serde(default)]
    pub contradictions: Vec<CanonicalRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ResearchQuestion {
    pub id: ResearchQuestionId,
    pub question: String,
    #[serde(default)]
    pub derived_from: Vec<CanonicalRef>,
    pub depth: u32,
    pub status: ResearchQuestionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_ref: Option<CanonicalRef>,
}
~~~

The implementation can initially retain existing Code Mode input types and add extended types only when schema and validator support lands.
