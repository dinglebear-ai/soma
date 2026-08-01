use std::{collections::HashSet, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use soma_provider_core::ProviderInvocationContext;

use super::{
    ConformanceAttestation, GraduationArtifact, GraduationState, MAX_FIXTURE_BYTES, WorkspaceLock,
    digest_bytes, digest_file, ensure_no_transaction, read_bounded, read_state,
    validate_state_paths, write_state,
};

const MAX_FIXTURE_VALUE_BYTES: usize = 128 * 1024;
const MAX_FIXTURES: usize = 64;
const MAX_COMPARISON_REPORT_BYTES: usize = 32 * 1024;

/// One recorded Python input/output pair used for component conformance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraduationFixture {
    /// Stable fixture name used in comparison reports.
    pub name: String,
    /// Fixture selector containing only provider, action, and arguments.
    pub input: Value,
    /// Expected JSON result produced by the Python provider.
    pub expected: Value,
}

struct PreparedComparison {
    state: GraduationState,
    candidate: GraduationArtifact,
    catalog_digest: String,
    component: soma_provider_adapters::wasm::PreparedComponentArtifact,
}

struct ComparisonOutcome {
    candidate: GraduationArtifact,
    fixture_digest: String,
    fixture_count: usize,
    matches: bool,
    source_sha256: String,
    catalog_sha256: String,
}

pub(crate) struct FixtureSnapshot {
    pub fixtures: Vec<GraduationFixture>,
    pub digest: String,
}

/// Host-owned inputs for one bounded Python/component dual-run.
pub struct ComparisonRequest<'a> {
    /// Optional candidate path, which must resolve to the published candidate.
    pub component: Option<&'a Path>,
    /// Immutable fixture snapshot used by both runtime executions.
    pub(crate) fixtures: FixtureSnapshot,
    /// Canonical host-owned input and live Python output for every fixture.
    pub live_runs: Vec<(Value, Value)>,
    /// Authenticated invocation context shared by both runtimes.
    pub context: &'a ProviderInvocationContext,
    /// Canonical managed provider root.
    pub provider_root: &'a Path,
    /// Absolute deadline covering preparation, execution, and persistence.
    pub deadline: tokio::time::Instant,
    /// Negotiated byte limit for the exact surface response envelope.
    pub max_response_bytes: usize,
}

/// Replay fixtures and persist digest-bound conformance evidence.
pub async fn compare(workspace: &Path, request: ComparisonRequest<'_>) -> anyhow::Result<Value> {
    let ComparisonRequest {
        component,
        fixtures,
        live_runs,
        context,
        provider_root,
        deadline,
        max_response_bytes,
    } = request;
    let workspace_for_prepare = workspace.to_path_buf();
    let component_for_prepare = component.map(Path::to_path_buf);
    let provider_root_for_prepare = provider_root.to_path_buf();
    let compile_deadline = deadline.into_std();
    let prepare_task = tokio::task::spawn_blocking(move || {
        prepare_comparison(
            &workspace_for_prepare,
            component_for_prepare.as_deref(),
            &provider_root_for_prepare,
            compile_deadline,
        )
    });
    let remaining = deadline
        .checked_duration_since(tokio::time::Instant::now())
        .ok_or_else(|| anyhow::anyhow!("graduation comparison exceeded its 30 second limit"))?;
    let prepared = tokio::time::timeout(remaining, prepare_task)
        .await
        .map_err(|_| anyhow::anyhow!("graduation comparison exceeded its 30 second limit"))???;
    if live_runs.len() != fixtures.fixtures.len() {
        anyhow::bail!("live Python result count does not match the fixture corpus");
    }
    let mut results = Vec::with_capacity(fixtures.fixtures.len());
    for (fixture, (effective_input, live_output)) in fixtures.fixtures.iter().zip(live_runs) {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .ok_or_else(|| anyhow::anyhow!("graduation comparison exceeded its 30 second limit"))?;
        let actual = tokio::time::timeout(
            remaining,
            soma_provider_adapters::wasm::invoke_prepared_component_artifact_before_async(
                &prepared.component,
                &effective_input,
                &prepared.state.catalog.capabilities,
                context,
                deadline.into_std(),
            ),
        )
        .await
        .map_err(|_| anyhow::anyhow!("graduation comparison exceeded its 30 second limit"))?;
        let component_matches_live = actual.as_ref().is_ok_and(|actual| actual == &live_output);
        let recorded_matches_live = fixture.expected == live_output;
        results.push(json!({
            "name": fixture.name.chars().take(64).collect::<String>(),
            "recorded_matches_live": recorded_matches_live,
            "component_matches_live": component_matches_live,
            "error": actual.as_ref().err().map(|error| error.chars().take(96).collect::<String>()),
        }));
    }
    let matches = results.iter().all(|result| {
        result["recorded_matches_live"] == true && result["component_matches_live"] == true
    });
    let report = json!({
        "ok": matches,
        "artifact_sha256": prepared.candidate.sha256,
        "source_sha256": prepared.state.source_sha256,
        "fixtures_sha256": fixtures.digest,
        "fixtures": results
    });
    if serde_json::to_vec(&report)?.len() > MAX_COMPARISON_REPORT_BYTES {
        anyhow::bail!("graduation comparison report exceeds {MAX_COMPARISON_REPORT_BYTES} bytes");
    }
    crate::ExecuteActionResponse {
        output: report.clone(),
        request_id: context.request_id.clone(),
        progress: context.progress.events(),
    }
    .enforce_serialized_limit(max_response_bytes)
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let workspace_for_finish = workspace.to_path_buf();
    let outcome = ComparisonOutcome {
        candidate: prepared.candidate.clone(),
        fixture_digest: fixtures.digest.clone(),
        fixture_count: fixtures.fixtures.len(),
        matches,
        source_sha256: prepared.state.source_sha256.clone(),
        catalog_sha256: prepared.catalog_digest.clone(),
    };
    let finish_deadline = deadline.into_std();
    let finish_task = tokio::task::spawn_blocking(move || {
        finish_comparison(&workspace_for_finish, &outcome, finish_deadline)
    });
    let remaining = deadline
        .checked_duration_since(tokio::time::Instant::now())
        .ok_or_else(|| anyhow::anyhow!("graduation comparison exceeded its 30 second limit"))?;
    tokio::time::timeout(remaining, finish_task)
        .await
        .map_err(|_| anyhow::anyhow!("graduation comparison exceeded its 30 second limit"))???;
    Ok(report)
}

fn prepare_comparison(
    workspace: &Path,
    component: Option<&Path>,
    provider_root: &Path,
    deadline: std::time::Instant,
) -> anyhow::Result<PreparedComparison> {
    let _lock = WorkspaceLock::acquire_before(workspace, deadline)?;
    ensure_no_transaction(workspace)?;
    let mut state = read_state(workspace)?;
    validate_state_paths(workspace, provider_root, &state)?;
    if digest_file(&state.source)? != state.source_sha256 {
        anyhow::bail!("live Python source changed since graduation was scaffolded");
    }
    let candidate = state
        .candidate
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no component candidate exists"))?;
    let component = component.unwrap_or(&candidate.path).canonicalize()?;
    if component != candidate.path.canonicalize()? {
        anyhow::bail!("comparison component is not the published candidate");
    }
    if digest_file(&candidate.path)? != candidate.sha256 {
        anyhow::bail!("component artifact digest mismatch");
    }
    // Starting a new comparison invalidates any prior proof immediately.
    // Timeouts, response-limit failures, or mismatches must never leave an
    // older candidate attestation available to activation.
    if state.attestation.take().is_some() {
        write_state(workspace, &state)?;
    }
    let component =
        soma_provider_adapters::wasm::prepare_component_artifact_before(&candidate.path, deadline)
            .map_err(anyhow::Error::msg)?;
    let catalog_digest = super::catalog_contract_digest(&state.catalog)?;
    Ok(PreparedComparison {
        state,
        candidate,
        catalog_digest,
        component,
    })
}

fn finish_comparison(
    workspace: &Path,
    outcome: &ComparisonOutcome,
    deadline: std::time::Instant,
) -> anyhow::Result<()> {
    let _lock = WorkspaceLock::acquire_before(workspace, deadline)?;
    ensure_no_transaction(workspace)?;
    let mut state = read_state(workspace)?;
    if state.candidate.as_ref() != Some(&outcome.candidate) {
        anyhow::bail!("graduation candidate changed while comparison was running");
    }
    if state.source_sha256 != outcome.source_sha256
        || digest_file(&state.source)? != outcome.source_sha256
    {
        anyhow::bail!("live Python source changed while comparison was running");
    }
    if state.catalog_sha256 != outcome.catalog_sha256
        || super::catalog_contract_digest(&state.catalog)? != outcome.catalog_sha256
    {
        anyhow::bail!("provider catalog changed while comparison was running");
    }
    if digest_file(&outcome.candidate.path)? != outcome.candidate.sha256 {
        anyhow::bail!("component candidate changed while comparison was running");
    }
    state.attestation = outcome.matches.then(|| ConformanceAttestation {
        artifact_sha256: outcome.candidate.sha256.clone(),
        fixtures_sha256: outcome.fixture_digest.clone(),
        fixture_count: outcome.fixture_count,
        source_sha256: outcome.source_sha256.clone(),
        catalog_sha256: outcome.catalog_sha256.clone(),
        verified_unix_ms: super::unix_ms(),
    });
    write_state(workspace, &state)
}

pub(crate) fn read_fixture_snapshot(path: &Path) -> anyhow::Result<FixtureSnapshot> {
    let bytes = read_bounded(path, MAX_FIXTURE_BYTES, "graduation fixtures")?;
    Ok(FixtureSnapshot {
        fixtures: parse_fixtures(&bytes)?,
        digest: digest_bytes(&bytes),
    })
}

pub(crate) fn read_fixtures(path: &Path) -> anyhow::Result<Vec<GraduationFixture>> {
    Ok(read_fixture_snapshot(path)?.fixtures)
}

fn parse_fixtures(bytes: &[u8]) -> anyhow::Result<Vec<GraduationFixture>> {
    let corpus: Vec<GraduationFixture> = serde_json::from_slice(bytes)?;
    if corpus.is_empty() {
        anyhow::bail!("graduation fixture corpus must not be empty");
    }
    if corpus.len() > MAX_FIXTURES {
        anyhow::bail!("graduation fixture corpus exceeds {MAX_FIXTURES} entries");
    }
    let mut names = HashSet::with_capacity(corpus.len());
    for fixture in &corpus {
        if fixture.name.is_empty() || fixture.name.len() > 256 {
            anyhow::bail!("graduation fixture names must contain 1 to 256 bytes");
        }
        if !names.insert(fixture.name.as_str()) {
            anyhow::bail!("graduation fixture names must be unique");
        }
        let input = fixture
            .input
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("graduation fixture input must be an object"))?;
        if !["provider", "action", "arguments"]
            .iter()
            .all(|key| input.contains_key(*key))
            || input
                .keys()
                .any(|key| !matches!(key.as_str(), "provider" | "action" | "arguments"))
        {
            anyhow::bail!(
                "graduation fixture input must contain only provider, action, and arguments"
            );
        }
        if serde_json::to_vec(&(&fixture.input, &fixture.expected))?.len() > MAX_FIXTURE_VALUE_BYTES
        {
            anyhow::bail!(
                "graduation fixture `{}` exceeds {MAX_FIXTURE_VALUE_BYTES} input/output bytes",
                fixture.name
            );
        }
    }
    Ok(corpus)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_a_nonempty_recorded_fixture_set() {
        assert!(
            parse_fixtures(b"[]")
                .expect_err("empty corpus")
                .to_string()
                .contains("must not be empty")
        );
    }

    #[test]
    fn corpus_and_entries_are_bounded() {
        assert!(parse_fixtures(&vec![b' '; MAX_FIXTURE_BYTES + 1]).is_err());

        let duplicate = serde_json::to_vec(&[
            GraduationFixture {
                name: "same".to_owned(),
                input: json!({"provider": "example", "action": "echo", "arguments": {}}),
                expected: json!({}),
            },
            GraduationFixture {
                name: "same".to_owned(),
                input: json!({"provider": "example", "action": "echo", "arguments": {}}),
                expected: json!({}),
            },
        ])
        .expect("fixture JSON");
        assert!(
            parse_fixtures(&duplicate)
                .expect_err("duplicates rejected")
                .to_string()
                .contains("unique")
        );

        let oversized = serde_json::to_vec(&[GraduationFixture {
            name: "large".to_owned(),
            input: json!({
                "provider": "example",
                "action": "echo",
                "arguments": {"value": "x".repeat(MAX_FIXTURE_VALUE_BYTES)}
            }),
            expected: json!({}),
        }])
        .expect("fixture JSON");
        assert!(
            parse_fixtures(&oversized)
                .expect_err("oversized value rejected")
                .to_string()
                .contains("input/output bytes")
        );
    }

    #[test]
    fn fixture_snapshot_is_immutable_after_the_source_file_changes() {
        let temp = tempfile::NamedTempFile::new().expect("fixture file");
        let first = serde_json::to_vec(&[GraduationFixture {
            name: "first".to_owned(),
            input: json!({"provider": "example", "action": "echo", "arguments": {}}),
            expected: json!({"value": 1}),
        }])
        .expect("fixture JSON");
        std::fs::write(temp.path(), &first).expect("first corpus");
        let snapshot = read_fixture_snapshot(temp.path()).expect("snapshot");
        std::fs::write(
            temp.path(),
            serde_json::to_vec(&[GraduationFixture {
                name: "second".to_owned(),
                input: json!({"provider": "example", "action": "echo", "arguments": {}}),
                expected: json!({"value": 2}),
            }])
            .expect("second fixture JSON"),
        )
        .expect("replace corpus");

        assert_eq!(snapshot.fixtures[0].name, "first");
        assert_eq!(snapshot.digest, digest_bytes(&first));
    }
}
