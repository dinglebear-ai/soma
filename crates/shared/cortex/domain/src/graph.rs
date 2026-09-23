use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphEntity {
    pub id: i64,
    pub entity_type: String,
    pub canonical_key: String,
    pub display_label: String,
    pub source_kind: String,
    pub source_id: String,
    pub trust_level: String,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphEntityCandidate {
    pub entity: GraphEntity,
    pub match_reason: String,
    pub alias_type: Option<String>,
    pub alias_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphRelationship {
    pub id: i64,
    pub relationship_key: String,
    pub src_entity_id: i64,
    pub dst_entity_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_entity: Option<GraphEntitySummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dst_entity: Option<GraphEntitySummary>,
    pub relationship_type: String,
    pub reason_code: String,
    pub trust_level: String,
    pub confidence: f64,
    pub evidence_count: i64,
    pub evidence_ids: Vec<i64>,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphEntitySummary {
    pub id: i64,
    pub entity_type: String,
    pub canonical_key: String,
    pub display_label: String,
    pub trust_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphEvidence {
    pub id: i64,
    pub relationship_id: i64,
    pub source_kind: String,
    pub source_id: String,
    pub source_log_id: Option<i64>,
    pub source_heartbeat_id: Option<i64>,
    pub source_signature_hash: Option<String>,
    pub observed_at: String,
    pub reason_code: String,
    pub reason_text: Option<String>,
    pub confidence_delta: f64,
    pub trust_level: String,
    pub safe_excerpt: Option<String>,
    pub metadata_path: Option<String>,
    pub evidence_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphSourceLogSummary {
    pub id: i64,
    pub timestamp: String,
    pub received_at: String,
    pub hostname: String,
    pub severity: String,
    pub app_name: Option<String>,
    pub process_id: Option<String>,
    pub source_ip: String,
    pub message: String,
    pub message_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphIncidentNarrative {
    pub title: String,
    pub summary: String,
    pub confidence: String,
    pub relationship_ids: Vec<i64>,
    pub evidence_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphNarrativeChain {
    pub chain_id: String,
    pub confidence: String,
    pub score: f64,
    pub summary: String,
    pub entities: Vec<GraphEntity>,
    pub relationships: Vec<GraphRelationship>,
    pub evidence_ids: Vec<i64>,
    pub relationship_ids: Vec<i64>,
    pub open_questions: Vec<String>,
}

impl From<&GraphEntity> for GraphEntitySummary {
    fn from(value: &GraphEntity) -> Self {
        Self {
            id: value.id,
            entity_type: value.entity_type.clone(),
            canonical_key: value.canonical_key.clone(),
            display_label: value.display_label.clone(),
            trust_level: value.trust_level.clone(),
        }
    }
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
