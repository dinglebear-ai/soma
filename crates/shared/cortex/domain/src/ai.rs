use crate::{HeartbeatWindowSummary, LogEntry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbuseIncident {
    pub incident_id: String,
    pub project: String,
    pub tool: String,
    pub session_id: String,
    pub hostname: String,
    pub first_seen: String,
    pub last_seen: String,
    pub duration_secs: i64,
    pub abuse_count: usize,
    pub terms: Vec<String>,
    pub anchor_ids: Vec<i64>,
    pub priority_score: f64,
    pub priority_label: String,
    pub window_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCorrelationAnchor {
    pub entry: LogEntry,
    pub window_from: String,
    pub window_to: String,
    pub related: Vec<LogEntry>,
    pub related_truncated: bool,
}

/// A graph entity a topic term resolved to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedTopicEntity {
    #[serde(rename = "type")]
    pub entity_type: String,
    pub key: String,
    /// How it matched: `exact`, `prefix`, `label`, or `alias`.
    pub match_kind: String,
    /// Resolver outcome: `resolved` for exact canonical-key and alias
    /// identity matches, `ambiguous` for weak label/prefix candidates that
    /// never drive log fan-out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolver_status: Option<String>,
}

/// An entity reached by graph expansion from the resolved seeds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicExpansionEntity {
    #[serde(rename = "type")]
    pub entity_type: String,
    pub key: String,
}

/// One unified-timeline row in a topic correlation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicTimelineEntry {
    pub timestamp: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    /// Discovery lane: `agent_command`, `shell_history`, or `graph:host:<host>`.
    pub entity_path: String,
    pub hostname: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Why this row is in the timeline (`service_instance`, `graph_related`,
    /// `host_context`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inclusion_reason: Option<String>,
    /// Resolver outcome for this row's inclusion path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolver_status: Option<String>,
    /// Set when the row was included by an explicit degraded fallback
    /// (`explicit_degraded_host_context`), never silently.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_kind: Option<String>,
}

/// One log row in a graph-anchored session correlation, annotated with how it
/// was reached and which source lane it belongs to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelatedLogRow {
    pub entry: LogEntry,
    /// Source kind parsed from the row (`agent-command`, `shell-history`,
    /// `syslog-udp`, `docker-stream`, ...); `None` if not recorded on the row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    /// How the graph traversal reached this row: `agent_command`,
    /// `shell_history`, or `graph:host:<hostname>`.
    pub discovery: String,
}

/// Graph-anchored, session-scoped correlation result for `ai_correlate`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSessionCorrelation {
    pub session_id: String,
    pub session_start: String,
    pub session_end: String,
    /// `true` when the session's `ai_session` graph entity was found and used to
    /// discover related hosts; `false` for the time-windowed fallback (session
    /// not yet projected into the graph).
    pub used_graph: bool,
    pub discovered_hosts: Vec<String>,
    pub discovered_entities: Vec<String>,
    pub logs: Vec<CorrelatedLogRow>,
    /// Count of agent-command rows (Claude's bash tool calls) in this session.
    pub agent_command_count: usize,
    /// Count of shell-history rows (the operator's own shell) in the window.
    pub shell_history_count: usize,
    /// Heartbeat pressure summaries for the discovered hosts over the window.
    pub heartbeat_summaries: Vec<HeartbeatWindowSummary>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookSignalCounts {
    pub hook_failed: usize,
    pub hook_timed_out: usize,
    pub hook_output_parse_error: usize,
    pub hook_invoked_too_often: usize,
    pub user_correction_after_hook: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookIncident {
    pub incident_id: String,
    pub hook_event: String,
    pub hook_name: Option<String>,
    pub hook_source: Option<String>,
    pub tool: String,
    pub project: String,
    pub session_id: String,
    pub hostname: String,
    pub first_seen: String,
    pub last_seen: String,
    pub duration_secs: i64,
    pub hook_event_count: usize,
    pub hook_event_ids: Vec<i64>,
    pub anchor_log_ids: Vec<i64>,
    pub signal_counts: HookSignalCounts,
    pub signals_present: Vec<String>,
    pub has_runtime_evidence: bool,
    pub priority_score: f64,
    pub priority_label: String,
    pub window_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookIncidentSummary {
    pub incident_id: String,
    pub first_seen: String,
    pub last_seen: String,
    pub priority_score: f64,
    pub priority_label: String,
    pub has_runtime_evidence: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpSignalCounts {
    pub repeated_call_failure: usize,
    pub timeout_or_rate_limit: usize,
    pub auth_or_permission_failure: usize,
    pub schema_or_validation_error: usize,
    pub unknown_tool_or_server: usize,
    pub user_correction_after_tool_call: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpIncident {
    pub incident_id: String,
    pub mcp_server: String,
    pub mcp_tool: Option<String>,
    pub tool: String,
    pub project: String,
    pub session_id: String,
    pub hostname: String,
    pub first_seen: String,
    pub last_seen: String,
    pub duration_secs: i64,
    pub event_count: usize,
    pub error_count: usize,
    pub mcp_event_ids: Vec<i64>,
    pub anchor_log_ids: Vec<i64>,
    pub signal_counts: McpSignalCounts,
    pub signals_present: Vec<String>,
    pub priority_score: f64,
    pub priority_label: String,
    pub window_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpIncidentSummary {
    pub incident_id: String,
    pub first_seen: String,
    pub last_seen: String,
    pub priority_score: f64,
    pub priority_label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillSignalCounts {
    pub user_correction_after_skill: usize,
    pub tool_failure_after_skill: usize,
    pub scope_or_source_confusion: usize,
    pub ignored_skill_or_policy_instruction: usize,
    pub overlong_loop_after_skill: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillIncident {
    pub incident_id: String,
    pub skill_name: String,
    pub skill_plugin: Option<String>,
    pub tool: String,
    pub project: String,
    pub session_id: String,
    pub hostname: String,
    pub first_seen: String,
    pub last_seen: String,
    pub duration_secs: i64,
    pub skill_event_count: usize,
    pub skill_event_ids: Vec<i64>,
    pub anchor_log_ids: Vec<i64>,
    pub signal_counts: SkillSignalCounts,
    pub signals_present: Vec<String>,
    pub priority_score: f64,
    pub priority_label: String,
    pub window_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillIncidentSummary {
    pub incident_id: String,
    pub first_seen: String,
    pub last_seen: String,
    pub priority_score: f64,
    pub priority_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEventEntry {
    pub id: i64,
    pub log_id: Option<i64>,
    pub ai_tool: String,
    pub ai_project: Option<String>,
    pub ai_session_id: Option<String>,
    pub hostname: String,
    pub timestamp: String,
    pub hook_event: String,
    pub hook_name: Option<String>,
    pub hook_source: Option<String>,
    pub hook_command: Option<String>,
    pub status: String,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub stdout_preview: Option<String>,
    pub stderr_preview: Option<String>,
    pub persisted_output_path: Option<String>,
    pub trusted_hash: Option<String>,
    pub evidence_kind: String,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpEventEntry {
    pub id: i64,
    pub call_log_id: Option<i64>,
    pub result_log_id: Option<i64>,
    pub ai_tool: String,
    pub ai_project: Option<String>,
    pub ai_session_id: Option<String>,
    pub hostname: String,
    pub timestamp: String,
    pub turn_id: Option<String>,
    pub call_id: String,
    pub tool_name: String,
    pub mcp_server: Option<String>,
    pub mcp_tool: Option<String>,
    pub event_kind: String,
    pub status: Option<String>,
    pub duration_ms: Option<i64>,
    pub is_error: Option<bool>,
    pub arguments_json: Option<String>,
    pub output_preview: Option<String>,
    pub error_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEventEntry {
    pub id: i64,
    pub log_id: i64,
    pub ai_tool: String,
    pub ai_project: Option<String>,
    pub ai_session_id: Option<String>,
    pub hostname: String,
    pub timestamp: String,
    pub skill_name: String,
    pub skill_plugin: Option<String>,
    pub event_kind: String,
    pub evidence_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSessionEntry {
    /// Stable response-local key for this host/tool/project/session tuple.
    pub session_key: String,
    pub project: String,
    pub tool: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    pub hostname: String,
    pub first_seen: String,
    pub last_seen: String,
    pub event_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchedSessionEntry {
    /// Stable response-local key for this host/tool/project/session tuple.
    pub session_key: String,
    pub project: String,
    pub tool: String,
    pub session_id: String,
    pub hostname: String,
    pub first_seen: String,
    pub last_seen: String,
    pub event_count: i64,
    pub match_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbuseMatch {
    pub term: String,
    pub entry: LogEntry,
    pub before: Vec<LogEntry>,
    pub after: Vec<LogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageBlock {
    pub bucket_start: String,
    pub bucket_end: String,
    pub project: String,
    pub tool: String,
    pub session_count: i64,
    pub event_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiToolEntry {
    pub tool: String,
    pub event_count: i64,
    pub session_count: i64,
    pub first_seen: String,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProjectEntry {
    pub project: String,
    pub tools: Vec<String>,
    pub event_count: i64,
    pub session_count: i64,
    pub first_seen: String,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelatedSession {
    pub session_id: String,
    pub project: String,
    pub tool: String,
    pub match_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentCluster {
    pub hostname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    pub window_start: String,
    pub window_end: String,
    pub log_count: i64,
    pub severity_peak: String,
    pub representative_messages: Vec<String>,
    pub correlated_sessions: Vec<CorrelatedSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityCount {
    pub severity: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppLogCount {
    pub app_name: Option<String>,
    pub count: i64,
}

#[cfg(test)]
#[path = "ai_tests.rs"]
mod tests;
