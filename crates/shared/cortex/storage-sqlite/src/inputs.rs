//! Storage-neutral input contracts for normalized AI event persistence.
//!
//! Scanner/runtime layers normalize untrusted transcript data before it reaches
//! this crate. SQLite accepts these bounded semantic values and persists them;
//! it does not own transcript parsing or scanner policy.

/// Runtime status of a normalized hook event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookStatus {
    Success,
    Failed,
    Blocked,
    Error,
    Unknown,
    Configured,
}

impl HookStatus {
    /// Stable persisted string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Error => "error",
            Self::Unknown => "unknown",
            Self::Configured => "configured",
        }
    }
}

/// Provenance category for a normalized hook event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvidenceKind {
    RuntimeTranscript,
    ConfigInventory,
    TrustedHashState,
    LogCorrelation,
    SideEffectInference,
}

impl HookEvidenceKind {
    /// Stable persisted string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RuntimeTranscript => "runtime_transcript",
            Self::ConfigInventory => "config_inventory",
            Self::TrustedHashState => "trusted_hash_state",
            Self::LogCorrelation => "log_correlation",
            Self::SideEffectInference => "side_effect_inference",
        }
    }
}

/// Already-normalized hook event accepted by SQLite persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedHookEvent {
    pub hook_event: String,
    pub hook_name: Option<String>,
    pub hook_source: Option<String>,
    pub hook_command: Option<String>,
    pub status: HookStatus,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub stdout_preview: Option<String>,
    pub stderr_preview: Option<String>,
    pub persisted_output_path: Option<String>,
    pub trusted_hash: Option<String>,
    pub evidence_kind: HookEvidenceKind,
    pub metadata_json: Option<String>,
}

/// Whether a normalized MCP event represents a call or its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpEventKind {
    Call,
    Result,
}

impl McpEventKind {
    /// Stable persisted string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::Result => "result",
        }
    }
}

/// Already-normalized MCP event accepted by SQLite persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedMcpEvent {
    pub call_id: String,
    pub tool_name: String,
    pub mcp_server: Option<String>,
    pub mcp_tool: Option<String>,
    pub event_kind: McpEventKind,
    pub turn_id: Option<String>,
    pub status: Option<String>,
    pub is_error: Option<bool>,
    pub arguments_json: Option<String>,
    pub output_preview: Option<String>,
    pub error_text: Option<String>,
}

/// Source shape that produced a normalized skill event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillEventKind {
    ClaudeAttribution,
    CodexSkillBlock,
}

impl SkillEventKind {
    /// Stable persisted string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeAttribution => "claude_attribution",
            Self::CodexSkillBlock => "codex_skill_block",
        }
    }
}

/// Evidence source for a normalized skill event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillEvidenceKind {
    StructuredJsonField,
    TranscriptContent,
}

impl SkillEvidenceKind {
    /// Stable persisted string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StructuredJsonField => "structured_json_field",
            Self::TranscriptContent => "transcript_content",
        }
    }
}

/// Already-normalized skill event accepted by SQLite persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedSkillEvent {
    pub skill_name: String,
    pub skill_plugin: Option<String>,
    pub event_kind: SkillEventKind,
    pub evidence_kind: SkillEvidenceKind,
}

#[cfg(test)]
#[path = "inputs_tests.rs"]
mod tests;
