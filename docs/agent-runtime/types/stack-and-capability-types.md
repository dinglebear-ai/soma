---
title: "Agent Stack and Capability Types"
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

# Stack and Capability Types

Proposed files: <code>agent_runtime/stack.rs</code> and <code>agent_runtime/capability.rs</code>.

~~~rust
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::ids::{CanonicalRef, SecretRef, ServiceId, Sha256Digest, StackId};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentStack {
    pub api_version: String,
    pub kind: String,
    pub metadata: StackMetadata,
    pub services: BTreeMap<String, AgentServiceSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StackMetadata {
    pub name: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub annotations: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentServiceSpec {
    pub agent: AgentSpec,
    pub context: ContextBinding,
    pub gateway: GatewayBinding,
    pub runtime: RuntimeSpec,
    #[serde(default)]
    pub snippets: Vec<SnippetRequirement>,
    #[serde(default)]
    pub skills: Vec<PackagePrimitiveRef>,
    #[serde(default)]
    pub disclosure: DisclosurePolicy,
    #[serde(default)]
    pub observability: ObservabilityPolicy,
    #[serde(default)]
    pub outputs: BTreeMap<String, OutputSpec>,
    #[serde(default)]
    pub lifecycle: LifecyclePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSpec {
    pub runtime: AgentRuntimeKind,
    pub mode: AgentMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageBinding>,
    #[serde(default)]
    pub prompts: BTreeMap<PromptStage, PackagePrimitiveRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub approval_policy: ApprovalPolicy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentRuntimeKind { CodexAppServer }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentMode { OneShot, Resident }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MutationClass {
    ReadOnly,
    ArtifactWrite,
    RepositoryWrite,
    RuntimeMutation,
    InfrastructureMutation,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CapabilityRequest {
    #[serde(default)]
    pub upstreams: BTreeSet<String>,
    #[serde(default)]
    pub tools: BTreeSet<String>,
    #[serde(default)]
    pub scopes: BTreeSet<String>,
    pub mutation_class: MutationClass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EffectiveCapabilities {
    pub generation: u64,
    pub upstreams: BTreeSet<String>,
    pub tools: BTreeSet<String>,
    pub scopes: BTreeSet<String>,
    pub mutation_class: MutationClass,
    pub policy_refs: Vec<CanonicalRef>,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RuntimeSpec {
    pub provider: RuntimeProvider,
    pub image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default)]
    pub profiles: Vec<String>,
    #[serde(default)]
    pub mounts: Vec<MountSpec>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub secrets: BTreeMap<String, SecretRef>,
    #[serde(default)]
    pub resources: ResourceLimits,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeProvider { Incus }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MountSpec {
    pub source: String,
    pub target: String,
    pub mode: MountMode,
    pub purpose: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MountMode { ReadOnly, ReadWrite, WriteOnly }
~~~

The full schema defines the omitted supporting structs. The Rust implementation SHOULD split them by ownership rather than placing the complete manifest in one large source file.
