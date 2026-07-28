//! Python-provider tests for the file-backed provider source: environment
//! preparation, candidate interpreter selection, and dependency fingerprinting.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use soma_provider_adapters::python::PythonInterpreter;
use tempfile::tempdir;

use crate::{capabilities::CapabilityBroker, provider_registry::ProviderRegistry};

use super::super::tests::tool_manifest;
use super::super::{FileProviderSource, PythonProviderEnvironmentPreparer};

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
    assert!(error
        .to_string()
        .contains("failed to prepare Python provider environment: environment unavailable"));
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
