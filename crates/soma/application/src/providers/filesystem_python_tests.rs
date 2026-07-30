//! Python-provider tests for the file-backed provider source: environment
//! preparation, candidate interpreter selection, and dependency fingerprinting.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use soma_provider_adapters::python::PythonInterpreter;
use tempfile::tempdir;

use crate::{
    ProviderAuthMode, ProviderCall, ProviderPrincipal, ProviderRequestLimits, ProviderSurface,
    capabilities::CapabilityBroker, provider_registry::ProviderRegistry,
};

use super::super::tests::tool_manifest;
use super::super::{FileProviderSource, PythonProviderEnvironmentPreparer};
use super::{
    collect_python_dependency_paths, immutable_python_source, python_tree_digest,
    verify_immutable_generation,
};

#[test]
fn fingerprint_changes_when_python_dependency_changes() {
    let temp = tempdir().expect("tempdir");
    let package = temp.path().join("helpers");
    fs::create_dir(&package).expect("create helper package");
    fs::write(package.join("__init__.py"), "").expect("write package init");
    fs::write(package.join("schema.py"), "ACTION = 'first'\n").expect("write schema");
    fs::write(
        temp.path().join("entry.py"),
        "from helpers.schema import ACTION\nPROVIDER = {'name': 'entry', 'kind': 'python'}\ndef tool():\n    return ACTION\n",
    )
    .expect("write provider entry");
    let source = FileProviderSource::new(temp.path());

    let first = source.fingerprint().expect("first fingerprint");
    fs::write(package.join("schema.py"), "ACTION = 'second'\n").expect("rewrite schema");
    let second = source.fingerprint().expect("second fingerprint");

    assert_ne!(first, second);
}

#[test]
fn immutable_snapshot_includes_adjacent_non_python_assets_and_reclaims_on_drop() {
    let temp = tempdir().expect("tempdir");
    let provider = temp.path().join("asset_provider.py");
    let asset = temp.path().join("schema.json");
    fs::write(
        &provider,
        "PROVIDER = {'name': 'assets', 'kind': 'python'}\n",
    )
    .expect("write provider");
    fs::write(&asset, r#"{"value":"first"}"#).expect("write asset");

    let first =
        immutable_python_source(temp.path(), &provider).expect("snapshot complete provider tree");
    let first_generation = first.path.parent().unwrap().parent().unwrap().to_path_buf();
    assert_eq!(
        fs::read_to_string(first.path.with_file_name("schema.json")).unwrap(),
        r#"{"value":"first"}"#
    );

    fs::write(&asset, r#"{"value":"second"}"#).expect("rewrite asset");
    let second =
        immutable_python_source(temp.path(), &provider).expect("snapshot updated provider tree");
    let second_generation = second
        .path
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    assert_ne!(first_generation, second_generation);
    assert!(first_generation.is_dir());
    drop(first);
    assert!(!first_generation.exists());
    assert!(second_generation.is_dir());
    drop(second);
    assert!(!second_generation.exists());
}

#[test]
fn mixed_snapshot_is_rejected_before_publication() {
    let source = tempdir().expect("source tempdir");
    fs::write(source.path().join("provider.py"), "VALUE = 'old'\n").expect("write provider");
    fs::write(source.path().join("asset.txt"), "old").expect("write asset");
    let mut paths = std::collections::BTreeSet::new();
    collect_python_dependency_paths(source.path(), &mut paths).expect("collect source tree");
    let digest = python_tree_digest(source.path(), &paths).expect("digest source tree");

    let staging = tempdir().expect("staging tempdir");
    let tree = staging.path().join("tree");
    fs::create_dir(&tree).expect("create staging tree");
    fs::copy(source.path().join("provider.py"), tree.join("provider.py")).expect("copy provider");
    fs::write(tree.join("asset.txt"), "new").expect("simulate mid-copy asset mutation");

    let error = verify_immutable_generation(staging.path(), &digest)
        .expect_err("mixed generation must fail closed before rename");
    assert!(error.to_string().contains("snapshot digest mismatch"));
}

#[derive(Clone)]
struct StubPythonEnvironmentPreparer {
    calls: Arc<AtomicUsize>,
    result: Result<PythonInterpreter, &'static str>,
}

impl StubPythonEnvironmentPreparer {
    fn prepared(interpreter: PythonInterpreter) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            result: Ok(interpreter),
        }
    }

    fn failing(message: &'static str) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            result: Err(message),
        }
    }
}

impl PythonProviderEnvironmentPreparer for StubPythonEnvironmentPreparer {
    fn prepare(&self, _provider_path: &Path) -> Result<PythonInterpreter, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.result.clone().map_err(str::to_owned)
    }

    fn validate_candidate(
        &self,
        _provider_path: &Path,
        _candidate: &soma_provider_adapters::python::materializer::PreparedPythonEnvironment,
    ) -> Result<PythonInterpreter, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.result.clone().map_err(str::to_owned)
    }
}

#[test]
fn python_environment_preparation_runs_before_catalog_introspection() {
    let temp = tempdir().expect("tempdir");
    fs::write(temp.path().join("candidate.py"), "not valid Python")
        .expect("write Python candidate");
    let preparer = StubPythonEnvironmentPreparer::failing("environment unavailable");
    let calls = preparer.calls.clone();
    let source =
        FileProviderSource::new(temp.path()).with_python_environment_preparer(Arc::new(preparer));

    let error = match source.load() {
        Ok(_) => panic!("preparation should fail"),
        Err(error) => error,
    };

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        error
            .to_string()
            .contains("failed to prepare Python provider environment: environment unavailable")
    );
    assert!(!error.to_string().contains("invalid Python provider"));
}

#[test]
fn python_environment_preparer_selects_the_candidate_interpreter() {
    let temp = tempdir().expect("tempdir");
    let provider = temp.path().join("candidate.py");
    fs::write(
        &provider,
        "PROVIDER = {'name': 'candidate', 'kind': 'python'}\n",
    )
    .expect("write Python candidate");
    let expected = PythonInterpreter::Prepared(
        PathBuf::from("cache")
            .join(".venv")
            .join("bin")
            .join("python"),
    );
    let preparer = StubPythonEnvironmentPreparer::prepared(expected.clone());
    let calls = preparer.calls.clone();
    let source =
        FileProviderSource::new(temp.path()).with_python_environment_preparer(Arc::new(preparer));

    let selected = source
        .python_interpreter(&provider)
        .expect("select prepared interpreter");

    assert_eq!(selected, expected);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn non_python_candidates_do_not_invoke_environment_preparation() {
    let temp = tempdir().expect("tempdir");
    fs::write(
        temp.path().join("stable.json"),
        tool_manifest("stable-provider", "stable_action", None),
    )
    .expect("write stable provider");
    let preparer = StubPythonEnvironmentPreparer::failing("must not run");
    let calls = preparer.calls.clone();
    let source =
        FileProviderSource::new(temp.path()).with_python_environment_preparer(Arc::new(preparer));

    let providers = source.load().expect("load non-Python provider");

    assert_eq!(providers.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn failed_python_candidate_refresh_keeps_previous_snapshot_active() {
    let temp = tempdir().expect("tempdir");
    fs::write(
        temp.path().join("stable.json"),
        tool_manifest("stable-provider", "stable_action", None),
    )
    .expect("write stable provider");
    let preparer = StubPythonEnvironmentPreparer::failing("candidate environment failed");
    let calls = preparer.calls.clone();
    let source =
        FileProviderSource::new(temp.path()).with_python_environment_preparer(Arc::new(preparer));
    let registry =
        ProviderRegistry::with_file_source(Vec::new(), CapabilityBroker::default_deny(), source)
            .expect("initial registry");
    let previous = registry.snapshot();
    let previous_actions = previous.action_names();

    fs::write(
        temp.path().join("candidate.py"),
        "PROVIDER = {'name': 'candidate', 'kind': 'python'}\n",
    )
    .expect("write changed Python candidate");

    let refreshed = registry
        .refresh_file_providers()
        .expect("refresh retains last valid snapshot");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(refreshed.fingerprint, previous.fingerprint);
    assert_eq!(refreshed.action_names(), previous_actions);
    assert_eq!(registry.snapshot().fingerprint, previous.fingerprint);
    assert_eq!(
        refreshed.provider_for_action("stable_action"),
        Some("stable-provider")
    );
}

#[tokio::test]
async fn refresh_bursts_publish_once_and_retained_generation_rolls_back() {
    let temp = tempdir().expect("tempdir");
    let manifest = temp.path().join("stable.json");
    fs::write(
        &manifest,
        tool_manifest("stable-provider", "stable_action", None),
    )
    .expect("write initial provider");
    let registry = ProviderRegistry::with_file_source_async(
        Vec::new(),
        CapabilityBroker::default_deny(),
        FileProviderSource::new(temp.path()),
    )
    .await
    .expect("initial registry");
    assert_eq!(
        registry.snapshot().provider_for_action("stable_action"),
        Some("stable-provider")
    );

    fs::write(
        &manifest,
        tool_manifest("stable-provider", "replacement_action", None),
    )
    .expect("rewrite provider");
    let first_registry = registry.clone();
    let second_registry = registry.clone();
    let (first, second) = tokio::join!(
        first_registry.refresh_file_providers_async(),
        second_registry.refresh_file_providers_async()
    );
    first.expect("first refresh");
    second.expect("coalesced refresh");

    let status = registry.python_generation_status();
    assert_eq!(status["active"]["generation_id"], 2);
    assert_eq!(status["rollback_candidates"].as_array().unwrap().len(), 1);
    assert_eq!(
        registry
            .snapshot()
            .provider_for_action("replacement_action"),
        Some("stable-provider")
    );

    let report = registry
        .rollback_python_generation(1)
        .await
        .expect("rollback retained generation");
    assert_eq!(report["restored_generation_id"], 1);
    assert_eq!(
        registry.snapshot().provider_for_action("stable_action"),
        Some("stable-provider")
    );
    assert!(
        registry
            .snapshot()
            .provider_for_action("replacement_action")
            .is_none()
    );
}

#[tokio::test]
async fn refresh_coalesces_a_second_change_arriving_during_debounce() {
    let temp = tempdir().expect("tempdir");
    let manifest = temp.path().join("stable.json");
    fs::write(
        &manifest,
        tool_manifest("stable-provider", "stable_action", None),
    )
    .expect("write initial provider");
    let registry = ProviderRegistry::with_file_source_async(
        Vec::new(),
        CapabilityBroker::default_deny(),
        FileProviderSource::new(temp.path()),
    )
    .await
    .expect("initial registry");

    fs::write(
        &manifest,
        tool_manifest("stable-provider", "intermediate_action", None),
    )
    .expect("write intermediate provider");
    let first_registry = registry.clone();
    let first = tokio::spawn(async move { first_registry.refresh_file_providers_async().await });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    fs::write(
        &manifest,
        tool_manifest("stable-provider", "settled_action", None),
    )
    .expect("write settled provider");
    let second_registry = registry.clone();
    let second = tokio::spawn(async move { second_registry.refresh_file_providers_async().await });

    first.await.unwrap().expect("first refresh");
    second.await.unwrap().expect("coalesced follow-up");

    assert_eq!(
        registry.snapshot().provider_for_action("settled_action"),
        Some("stable-provider")
    );
    assert!(
        registry
            .snapshot()
            .provider_for_action("intermediate_action")
            .is_none()
    );
    let status = registry.python_generation_status();
    assert_eq!(status["active"]["generation_id"], 2);
    assert_eq!(status["rollback_candidates"].as_array().unwrap().len(), 1);
}

fn python_call(action: &str) -> ProviderCall {
    ProviderCall {
        provider: String::new(),
        action: action.to_owned(),
        params: serde_json::json!({}),
        principal: ProviderPrincipal::loopback_dev(),
        auth_mode: ProviderAuthMode::LoopbackDev,
        surface: ProviderSurface::Rest,
        destructive_confirmed: false,
        limits: ProviderRequestLimits::default(),
        snapshot_id: String::new(),
    }
}

#[tokio::test]
async fn rollback_executes_immutable_python_source_until_a_new_edit() {
    let temp = tempdir().expect("tempdir");
    let provider = temp.path().join("versioned.py");
    let source = |value: &str| {
        format!(
            "PROVIDER = {{'name': 'versioned', 'kind': 'python'}}\n\
             def version():\n    return {{'value': '{value}'}}\n"
        )
    };
    fs::write(&provider, source("old")).expect("write initial provider");
    let registry = ProviderRegistry::with_file_source_async(
        Vec::new(),
        CapabilityBroker::default_deny(),
        FileProviderSource::new(temp.path()),
    )
    .await
    .expect("initial registry");
    assert_eq!(
        registry
            .dispatch(python_call("version"))
            .await
            .expect("invoke old source")
            .value,
        serde_json::json!({"value": "old"})
    );

    fs::write(&provider, source("new")).expect("rewrite provider");
    registry
        .refresh_file_providers_async()
        .await
        .expect("publish new source");
    assert_eq!(
        registry
            .dispatch(python_call("version"))
            .await
            .expect("invoke new source")
            .value,
        serde_json::json!({"value": "new"})
    );

    registry
        .rollback_python_generation(1)
        .await
        .expect("rollback old source");
    assert_eq!(
        registry
            .dispatch(python_call("version"))
            .await
            .expect("invoke rolled-back source")
            .value,
        serde_json::json!({"value": "old"})
    );
    registry
        .refresh_file_providers_async()
        .await
        .expect("unchanged disk state stays pinned");
    assert_eq!(
        registry
            .dispatch(python_call("version"))
            .await
            .expect("invoke pinned rolled-back source")
            .value,
        serde_json::json!({"value": "old"})
    );

    fs::write(&provider, source("third")).expect("write third source");
    registry
        .refresh_file_providers_async()
        .await
        .expect("publish third source");
    assert_eq!(
        registry
            .dispatch(python_call("version"))
            .await
            .expect("invoke third source")
            .value,
        serde_json::json!({"value": "third"})
    );
}

#[tokio::test]
async fn python_provider_reads_adjacent_asset_from_immutable_generation() {
    let temp = tempdir().expect("tempdir");
    fs::write(temp.path().join("message.txt"), "immutable asset").expect("write asset");
    fs::write(
        temp.path().join("asset_provider.py"),
        r#"from pathlib import Path
PROVIDER = {'name': 'asset-provider', 'kind': 'python'}
def read_asset():
    return {'value': Path(__file__).with_name('message.txt').read_text()}
"#,
    )
    .expect("write provider");
    let registry = ProviderRegistry::with_file_source_async(
        Vec::new(),
        CapabilityBroker::default_deny(),
        FileProviderSource::new(temp.path()),
    )
    .await
    .expect("load provider with asset");

    let output = registry
        .dispatch(python_call("read_asset"))
        .await
        .expect("execute provider from immutable tree");
    assert_eq!(
        output.value,
        serde_json::json!({"value": "immutable asset"})
    );
}
