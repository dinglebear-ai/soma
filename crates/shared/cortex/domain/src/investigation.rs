use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvestigationBudget {
    pub max_graph_calls: u32,
    pub max_log_rows: u32,
    pub max_evidence_rows: u32,
    pub max_candidate_explanations: u32,
    pub max_wall_time_ms: u32,
    pub max_payload_bytes: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvestigationBudgetUsed {
    pub graph_calls: u32,
    pub log_rows: u32,
    pub evidence_rows: u32,
    pub candidate_explanations: u32,
    pub wall_time_ms: u32,
    pub payload_bytes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationClaimType {
    Verified,
    SupportedCorrelation,
    WeakCorrelation,
    OpenQuestion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvestigationClaim {
    pub claim_type: InvestigationClaimType,
    pub title: String,
    pub summary: String,
    pub confidence: String,
    pub relationship_ids: Vec<i64>,
    pub evidence_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppEntitySummary {
    pub id: i64,
    pub entity_type: String,
    pub key: String,
    pub label: String,
    pub trust_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppRelationshipSummary {
    pub id: i64,
    pub source_entity_id: i64,
    pub target_entity_id: i64,
    pub relationship_type: String,
    pub reason_code: String,
    pub trust_level: String,
    pub confidence: f64,
    pub evidence_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppEvidenceSummary {
    pub id: i64,
    pub relationship_id: i64,
    pub source_kind: String,
    pub source_log_id: Option<i64>,
    pub observed_at: String,
    pub reason_code: String,
    pub reason_text: Option<String>,
    pub confidence_delta: f64,
    pub trust_level: String,
    pub excerpt: Option<String>,
    pub missing_source_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppLogSummary {
    pub id: i64,
    pub timestamp: String,
    pub received_at: String,
    pub hostname: String,
    pub severity: String,
    pub app_name: Option<String>,
    pub message: String,
    pub message_truncated: bool,
}

#[cfg(test)]
#[path = "investigation_tests.rs"]
mod tests;
