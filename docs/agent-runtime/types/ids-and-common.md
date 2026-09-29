---
title: "Agent Runtime IDs and Common Types"
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

# IDs and Common Types

Proposed file: <code>crates/soma/domain/src/agent_runtime/ids.rs</code>.

~~~rust
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

macro_rules! string_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, AgentRuntimeTypeError> {
                let value = value.into();
                validate_id($label, &value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str { &self.0 }
            pub fn into_inner(self) -> String { self.0 }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = AgentRuntimeTypeError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

string_id!(StackId, "stack_id");
string_id!(ServiceId, "service_id");
string_id!(RunId, "run_id");
string_id!(AgentId, "agent_id");
string_id!(RuntimeInstanceId, "runtime_instance_id");
string_id!(ContextId, "context_id");
string_id!(ContextGenerationId, "context_generation_id");
string_id!(DisclosureRequestId, "disclosure_request_id");
string_id!(DisclosureDecisionId, "disclosure_decision_id");
string_id!(SnippetId, "snippet_id");
string_id!(SnippetExecutionId, "snippet_execution_id");
string_id!(ClaimId, "claim_id");
string_id!(ResearchQuestionId, "research_question_id");
string_id!(ArtifactId, "artifact_id");
string_id!(LifecycleEventId, "lifecycle_event_id");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalRef {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<Sha256Digest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn new(value: impl Into<String>) -> Result<Self, AgentRuntimeTypeError> {
        let value = value.into();
        let valid = value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit());
        valid.then(|| Self(value.to_ascii_lowercase())).ok_or_else(||
            AgentRuntimeTypeError::InvalidDigest)
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretRef {
    pub provider: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Sensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceClass {
    Observed,
    Verified,
    Documented,
    Implemented,
    Historical,
    Claimed,
    Inferred,
    Correlated,
    Contradicted,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRuntimeTypeError {
    #[error("{field} is invalid")]
    InvalidId { field: &'static str },
    #[error("SHA-256 digest is invalid")]
    InvalidDigest,
}

fn validate_id(field: &'static str, value: &str) -> Result<(), AgentRuntimeTypeError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    valid.then_some(()).ok_or(AgentRuntimeTypeError::InvalidId { field })
}
~~~

The domain crate already depends on Serde. Adding <code>thiserror</code> is optional; the implementation may instead follow the current domain error style in <code>crates/soma/domain/src/errors.rs</code>.
