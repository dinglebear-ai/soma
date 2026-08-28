//! Pure confidence math for the investigation graph.
//!
//! All functions here are pure (no DB, no clock): they take stored values and
//! return derived confidences. Temporal decay and evidence combination are
//! applied at *query time* on top of the stored peak confidence — nothing here
//! mutates persisted data, so there is no schema impact.
//!
//! Three ideas:
//! - **Noisy-OR** combines confidences from *independent* sources:
//!   `1 - product(1 - c_i)`. Monotonic, bounded `[0,1]`, rewards corroboration.
//! - **BEWA diminishing returns** collapses *same-source* repetition: 1000
//!   syslog lines are one fact seen 1000 times, not 1000 independent facts.
//!   Each doubling of `evidence_count` adds one effective observation.
//! - **CountTRuCoLa-style temporal decay** ages edges toward a floor with a
//!   per-relationship half-life, so stale `runs_on` edges fade while structural
//!   `worked_on` edges persist.

const REASON_AGENT_COMMAND_CWD_INFER: &str = "agent_command_cwd_infer";
const REASON_AGENT_COMMAND_GIT_COMMIT: &str = "agent_command_git_commit";
const REASON_AGENT_COMMAND_SESSION: &str = "agent_command_session";
const REASON_AI_SESSION_PROJECT: &str = "ai_session_project";
const REASON_COMPOSE_CONFIG: &str = "compose_config";
const REASON_DOCKER_CONTAINER_ID: &str = "docker_container_id";
const REASON_DOCKER_NETWORK: &str = "docker_network";
const REASON_DOCKER_SERVICE_LABEL: &str = "docker_service_label";
const REASON_ERROR_SIGNATURE_MATCH: &str = "error_signature_match";
const REASON_HEARTBEAT_HOST_STATE: &str = "heartbeat_host_state";
const REASON_LOG_APP_NAME: &str = "log_app_name";
const REASON_REVERSE_PROXY_CONFIG: &str = "reverse_proxy_config";
const REASON_SHELL_HISTORY_GIT_COMMIT: &str = "shell_history_git_commit";
const REASON_SYSLOG_CLAIMED_HOSTNAME: &str = "syslog_claimed_hostname";
const TRUST_CORRELATED: &str = "correlated";
const TRUST_REFUTED: &str = "refuted";

/// ln(2), the half-life constant for an exponential `exp(-lambda*t)` decay.
const LN2: f64 = std::f64::consts::LN_2;

/// Effective-confidence ceiling for `correlated`-trust edges. `correlated` marks
/// a derivation *method* (temporal co-occurrence), not a verified fact, so its
/// confidence is capped well below structural edges.
pub const TRUST_CORRELATED_CEILING: f64 = 0.5;

/// Cap a confidence by trust level: `refuted` edges contribute nothing,
/// `correlated` edges are capped at `TRUST_CORRELATED_CEILING`, everything else
/// passes through. Use after computing effective confidence.
pub fn apply_trust_ceiling(confidence: f64, trust_level: &str) -> f64 {
    match trust_level {
        TRUST_REFUTED => 0.0,
        TRUST_CORRELATED => confidence.min(TRUST_CORRELATED_CEILING),
        _ => confidence,
    }
}

/// Combine independent confidences via noisy-OR: `1 - product(1 - c_i)`.
///
/// A single source is returned unchanged; corroborating sources push the result
/// up toward (never past) 1.0. Inputs are clamped to `[0, 1]`; an empty slice
/// yields 0.0.
pub fn noisy_or_combine(confidences: &[f64]) -> f64 {
    let product = confidences
        .iter()
        .map(|c| 1.0 - c.clamp(0.0, 1.0))
        .product::<f64>();
    (1.0 - product).clamp(0.0, 1.0)
}

/// BEWA diminishing returns: the effective independent-observation count implied
/// by a raw same-source `evidence_count`. `log2(1 + n)` — each doubling of
/// same-source evidence adds one effective unit (1→1, 1000→~10).
pub fn bewa_effective_count(evidence_count: i64) -> f64 {
    if evidence_count <= 0 {
        return 0.0;
    }
    (1.0 + evidence_count as f64).ln() / LN2
}

/// Confidence accumulated from `evidence_count` same-source observations, each
/// of `per_observation` confidence, with BEWA diminishing returns folded into a
/// noisy-OR: `1 - (1 - p)^effective_count`.
pub fn confidence_from_repeated(per_observation: f64, evidence_count: i64) -> f64 {
    let p = per_observation.clamp(0.0, 1.0);
    let n = bewa_effective_count(evidence_count);
    (1.0 - (1.0 - p).powf(n)).clamp(0.0, 1.0)
}

/// Per-hour decay rate `lambda = ln2 / half_life_hours` for a reason code. `0` means
/// the edge never decays (structural facts like session→project).
pub fn decay_lambda_per_hour(reason_code: &str) -> f64 {
    let half_life_hours = match reason_code {
        // Volatile runtime topology — a container's host can change on restart.
        REASON_DOCKER_CONTAINER_ID | REASON_DOCKER_SERVICE_LABEL => 0.25,
        // Recent observations that age over a day.
        REASON_LOG_APP_NAME | REASON_SYSLOG_CLAIMED_HOSTNAME => 24.0,
        // Point-in-time signals decay fast.
        REASON_ERROR_SIGNATURE_MATCH | REASON_HEARTBEAT_HOST_STATE => 1.0,
        // Config-derived structure is stable for weeks.
        REASON_COMPOSE_CONFIG | REASON_REVERSE_PROXY_CONFIG | REASON_DOCKER_NETWORK => 720.0,
        // Structural / FK-backed facts never decay.
        REASON_AI_SESSION_PROJECT
        | REASON_AGENT_COMMAND_SESSION
        | REASON_AGENT_COMMAND_CWD_INFER
        | REASON_AGENT_COMMAND_GIT_COMMIT
        | REASON_SHELL_HISTORY_GIT_COMMIT => return 0.0,
        // Default: slow weekly decay for anything unlisted.
        _ => 168.0,
    };
    LN2 / half_life_hours
}

/// Asymptotic confidence floor `phi` for a reason code — the minimum the edge
/// decays toward as `delta_t -> infinity`. Point-in-time signals fall to 0; most edges keep
/// a small residual.
pub fn asymptotic_floor(reason_code: &str) -> f64 {
    match reason_code {
        REASON_ERROR_SIGNATURE_MATCH | REASON_HEARTBEAT_HOST_STATE => 0.0,
        _ => 0.1,
    }
}

/// Recency factor in `[phi, 1]`: `phi + (1 - phi) * exp(-lambda * delta_t)`.
/// `lambda = 0` (never-decay edges) returns exactly 1.0; `delta_t <= 0` returns 1.0.
pub fn compute_recency(lambda_per_hour: f64, delta_hours: f64, phi: f64) -> f64 {
    if lambda_per_hour <= 0.0 || delta_hours <= 0.0 {
        return 1.0;
    }
    let phi = phi.clamp(0.0, 1.0);
    phi + (1.0 - phi) * (-lambda_per_hour * delta_hours).exp()
}

/// Query-time effective confidence: `stored * recency(reason_code, delta_t)`.
/// `delta_hours` is `(now - last_seen_at)` in hours, computed by the caller.
pub fn compute_effective_confidence(stored: f64, reason_code: &str, delta_hours: f64) -> f64 {
    let lambda = decay_lambda_per_hour(reason_code);
    let phi = asymptotic_floor(reason_code);
    stored.clamp(0.0, 1.0) * compute_recency(lambda, delta_hours, phi)
}

#[cfg(test)]
#[path = "graph_confidence_tests.rs"]
mod tests;
