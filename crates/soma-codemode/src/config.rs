#![allow(dead_code)]

use serde::{Deserialize, Serialize};

pub const SERVICE: &str = "code_mode";
pub const MAX_SOURCE_BYTES: usize = 20_000;
pub(crate) const MAX_SNIPPET_RESOLVES_PER_RUN: usize = 32;
pub(crate) const MAX_INTERNAL_CALLS_PER_RUN: usize = 32;
pub(crate) const MAX_SNIPPET_RESOLVED_BYTES_PER_RUN: usize = 256 * 1024;

const DEFAULT_MAX_CALLTOOL_PER_RUN: u64 = 512;
const MAX_CALLTOOL_PER_RUN_CEILING: u64 = 2048;
const DEFAULT_CALLTOOL_RESULT_MAX_MIB: usize = 8;

static MAX_CALLS_PER_RUN_CONFIG_DEFAULT: std::sync::OnceLock<Option<u64>> =
    std::sync::OnceLock::new();
static CALLTOOL_RESULT_MAX_MIB_CONFIG_DEFAULT: std::sync::OnceLock<Option<usize>> =
    std::sync::OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CodeModeResultShapePolicy {
    #[default]
    Off,
    Truncate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticSearchConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tei_url: Option<String>,
    #[serde(default = "default_semantic_search_blend_weight")]
    pub blend_weight: f32,
}

impl Default for SemanticSearchConfig {
    fn default() -> Self {
        Self {
            tei_url: None,
            blend_weight: default_semantic_search_blend_weight(),
        }
    }
}

impl SemanticSearchConfig {
    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.tei_url
            .as_deref()
            .is_some_and(|url| !url.trim().is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeModeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub trace_params: bool,
    #[serde(default)]
    pub result_shape_policy: CodeModeResultShapePolicy,
    #[serde(default = "default_code_mode_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_code_mode_max_response_bytes")]
    pub max_response_bytes: usize,
    #[serde(default = "default_code_mode_max_response_tokens")]
    pub max_response_tokens: usize,
    #[serde(default = "default_token_estimate_divisor")]
    pub token_estimate_divisor: u32,
    #[serde(default = "default_max_log_entries")]
    pub max_log_entries: usize,
    #[serde(default = "default_max_log_bytes")]
    pub max_log_bytes: usize,
    #[serde(default)]
    pub semantic_search: SemanticSearchConfig,
    #[serde(default)]
    pub max_calls_per_run: Option<u64>,
    #[serde(default)]
    pub calltool_result_max_mib: Option<usize>,
}

impl Default for CodeModeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            trace_params: true,
            result_shape_policy: CodeModeResultShapePolicy::Off,
            timeout_ms: default_code_mode_timeout_ms(),
            max_response_bytes: default_code_mode_max_response_bytes(),
            max_response_tokens: default_code_mode_max_response_tokens(),
            token_estimate_divisor: default_token_estimate_divisor(),
            max_log_entries: default_max_log_entries(),
            max_log_bytes: default_max_log_bytes(),
            semantic_search: SemanticSearchConfig::default(),
            max_calls_per_run: None,
            calltool_result_max_mib: None,
        }
    }
}

impl CodeModeConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=60_000).contains(&self.timeout_ms) {
            return Err("timeout_ms must be between 1 and 60000".into());
        }
        if !(1024..=1024 * 1024).contains(&self.max_response_bytes) {
            return Err("max_response_bytes must be between 1024 and 1048576".into());
        }
        if !(256..=256_000).contains(&self.max_response_tokens) {
            return Err("max_response_tokens must be between 256 and 256000".into());
        }
        if !(1..=64).contains(&self.token_estimate_divisor) {
            return Err("token_estimate_divisor must be between 1 and 64".into());
        }
        if !(0.0..=1.0).contains(&self.semantic_search.blend_weight) {
            return Err("semantic_search.blend_weight must be between 0 and 1".into());
        }
        Ok(())
    }
}

pub fn install_call_budget_config_defaults(
    max_calls_per_run: Option<u64>,
    calltool_result_max_mib: Option<usize>,
) {
    let _ = MAX_CALLS_PER_RUN_CONFIG_DEFAULT.set(max_calls_per_run);
    let _ = CALLTOOL_RESULT_MAX_MIB_CONFIG_DEFAULT.set(calltool_result_max_mib);
}

pub(crate) fn max_calltool_per_run() -> u64 {
    let Some(raw) = crate::home::env_non_empty("SOMA_CODE_MODE_MAX_CALLS_PER_RUN") else {
        return MAX_CALLS_PER_RUN_CONFIG_DEFAULT
            .get()
            .copied()
            .flatten()
            .filter(|value| *value > 0)
            .map_or(DEFAULT_MAX_CALLTOOL_PER_RUN, |value| {
                value.min(MAX_CALLTOOL_PER_RUN_CEILING)
            });
    };
    raw.trim()
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .map_or(DEFAULT_MAX_CALLTOOL_PER_RUN, |value| {
            value.min(MAX_CALLTOOL_PER_RUN_CEILING)
        })
}

pub(crate) fn calltool_result_max_bytes() -> usize {
    let default_bytes = CALLTOOL_RESULT_MAX_MIB_CONFIG_DEFAULT
        .get()
        .copied()
        .flatten()
        .filter(|mib| *mib > 0)
        .map(|mib| mib.saturating_mul(1024 * 1024))
        .unwrap_or(DEFAULT_CALLTOOL_RESULT_MAX_MIB * 1024 * 1024);
    crate::home::env_non_empty("SOMA_CODE_MODE_CALLTOOL_RESULT_MAX_MIB")
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|mib| *mib > 0)
        .map(|mib| mib.saturating_mul(1024 * 1024))
        .unwrap_or(default_bytes)
}

fn default_true() -> bool {
    true
}

fn default_code_mode_timeout_ms() -> u64 {
    30_000
}

fn default_code_mode_max_response_bytes() -> usize {
    24 * 1024
}

fn default_code_mode_max_response_tokens() -> usize {
    6_000
}

fn default_token_estimate_divisor() -> u32 {
    4
}

fn default_max_log_entries() -> usize {
    1000
}

fn default_max_log_bytes() -> usize {
    65_536
}

fn default_semantic_search_blend_weight() -> f32 {
    0.5
}
