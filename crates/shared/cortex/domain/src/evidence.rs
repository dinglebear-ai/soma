use crate::{
    AbuseIncident, HookEventEntry, HookIncident, LogEntry, McpEventEntry, McpIncident,
    SkillEventEntry, SkillIncident, hook_incident_findings, incident_findings,
    mcp_incident_findings, skill_incident_findings,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentEvidence {
    pub incident: AbuseIncident,
    pub transcript_before: Vec<LogEntry>,
    pub transcript_before_truncated: bool,
    pub transcript_after: Vec<LogEntry>,
    pub transcript_after_truncated: bool,
    pub anchors: Vec<LogEntry>,
    pub nearby_logs: Vec<LogEntry>,
    pub nearby_logs_truncated: bool,
    pub nearby_errors: Vec<LogEntry>,
    /// Deterministic, rule-based failure hypotheses and prevention hints
    /// derived from this bundle (bead kmib.4). Never an LLM summary -- see
    /// [`crate::incident_findings`].
    pub findings: incident_findings::IncidentFindings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAssessEvidenceSummary {
    pub total_incidents: usize,
    pub evidence_bundle_count: usize,
    pub total_anchors: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookIncidentEvidence {
    pub incident: HookIncident,
    pub hook_events: Vec<HookEventEntry>,
    pub hook_events_truncated: bool,
    pub signal_anchors: Vec<LogEntry>,
    pub signal_anchors_truncated: bool,
    pub transcript_before: Vec<LogEntry>,
    pub transcript_before_truncated: bool,
    pub transcript_after: Vec<LogEntry>,
    pub transcript_after_truncated: bool,
    pub nearby_tool_calls: Vec<LogEntry>,
    pub nearby_tool_calls_truncated: bool,
    pub nearby_logs: Vec<LogEntry>,
    pub nearby_logs_truncated: bool,
    pub nearby_errors: Vec<LogEntry>,
    pub nearby_errors_truncated: bool,
    /// Deterministic, rule-based findings. Never an LLM summary -- see
    /// [`crate::hook_incident_findings`].
    pub findings: hook_incident_findings::HookIncidentFindings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpIncidentEvidence {
    pub incident: McpIncident,
    pub mcp_events: Vec<McpEventEntry>,
    pub mcp_events_truncated: bool,
    pub signal_anchors: Vec<LogEntry>,
    pub signal_anchors_truncated: bool,
    pub transcript_before: Vec<LogEntry>,
    pub transcript_before_truncated: bool,
    pub transcript_after: Vec<LogEntry>,
    pub transcript_after_truncated: bool,
    pub nearby_user_corrections: Vec<LogEntry>,
    pub nearby_user_corrections_truncated: bool,
    pub nearby_logs: Vec<LogEntry>,
    pub nearby_logs_truncated: bool,
    pub nearby_errors: Vec<LogEntry>,
    pub nearby_errors_truncated: bool,
    /// Deterministic, rule-based findings. Never an LLM summary -- see
    /// [`crate::mcp_incident_findings`].
    pub findings: mcp_incident_findings::McpIncidentFindings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillIncidentEvidence {
    pub incident: SkillIncident,
    pub skill_events: Vec<SkillEventEntry>,
    pub skill_events_truncated: bool,
    pub signal_anchors: Vec<LogEntry>,
    pub signal_anchors_truncated: bool,
    pub transcript_before: Vec<LogEntry>,
    pub transcript_before_truncated: bool,
    pub transcript_after: Vec<LogEntry>,
    pub transcript_after_truncated: bool,
    pub nearby_tool_failures: Vec<LogEntry>,
    pub nearby_tool_failures_truncated: bool,
    pub nearby_user_corrections: Vec<LogEntry>,
    pub nearby_user_corrections_truncated: bool,
    pub nearby_logs: Vec<LogEntry>,
    pub nearby_logs_truncated: bool,
    pub nearby_errors: Vec<LogEntry>,
    pub nearby_errors_truncated: bool,
    /// Deterministic, rule-based findings. Never an LLM summary -- see
    /// [`crate::skill_incident_findings`].
    pub findings: skill_incident_findings::SkillIncidentFindings,
}

/// One assessed incident's result (LLM assessment is `None` when the
/// caller requested deterministic-findings-only, e.g. `--no-llm` or an
/// MCP/REST caller).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookAssessResult {
    pub incident_id: String,
    pub findings: hook_incident_findings::HookIncidentFindings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assessment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_preview: Option<String>,
}

/// One assessed incident's result (LLM assessment is `None` when the
/// caller requested deterministic-findings-only, e.g. `--no-llm` or an
/// MCP/REST caller).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpAssessResult {
    pub incident_id: String,
    pub findings: mcp_incident_findings::McpIncidentFindings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assessment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_preview: Option<String>,
}

/// One assessed incident's result (LLM assessment is `None` when the
/// caller requested deterministic-findings-only, e.g. `--no-llm` or an
/// MCP/REST caller -- see `src/cli/commands/assess.rs` and
/// `src/app/services/skill_assessment.rs`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAssessResult {
    pub incident_id: String,
    pub findings: skill_incident_findings::SkillIncidentFindings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assessment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelatedHost {
    pub hostname: String,
    pub event_count: usize,
    pub events: Vec<LogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSignatureEntry {
    pub signature_hash: String,
    pub template: String,
    pub sample_message: String,
    pub severity: String,
    pub sample_hostname: String,
    pub sample_app_name: Option<String>,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub total_count: i64,
    pub count_last_1h: i64,
    pub acknowledged_at: Option<String>,
}

#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;
