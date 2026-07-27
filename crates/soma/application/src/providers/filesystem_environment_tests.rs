use std::{
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde_json::json;
use sha2::{Digest, Sha256};
use soma_provider_adapters::python::{
    environment::PythonRuntimeFingerprint,
    lifecycle::{PythonEnvironmentLifecycle, PythonEnvironmentSpec},
    materializer::UvRunner,
};
use tempfile::{tempdir, TempDir};

use crate::provider_registry::{
    Provider, ProviderAuthMode, ProviderCall, ProviderPrincipal, ProviderRequestLimits,
    ProviderSurface,
};

use super::super::FileProviderSource;

#[derive(Clone, Default)]
struct ExecutableUv {
    calls: Arc<Mutex<Vec<Vec<OsString>>>>,
    projects: Arc<Mutex<Vec<String>>>,
    python_path: Option<PathBuf>,
}

impl ExecutableUv {
    fn with_python_path(path: impl Into<PathBuf>) -> Self {
        Self {
            python_path: Some(path.into()),
            ..Self::default()
        }
    }

    fn operations(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter_map(|args| args.first().and_then(|arg| arg.to_str()).map(str::to_owned))
            .collect()
    }

    fn project(&self) -> String {
        self.projects.lock().unwrap()[0].clone()
    }
}

impl UvRunner for ExecutableUv {
    fn run(&self, _program: &Path, args: &[OsString], current_dir: &Path) -> Result<(), String> {
        self.calls.lock().unwrap().push(args.to_vec());
        match args.first().and_then(|arg| arg.to_str()) {
            Some("lock") => {
                self.projects
                    .lock()
                    .unwrap()
                    .push(fs::read_to_string(current_dir.join("pyproject.toml")).unwrap());
                fs::write(current_dir.join("uv.lock"), "version = 1").unwrap();
            }
            Some("sync") => self.write_python_launcher(current_dir)?,
            Some("pip") => {}
            other => return Err(format!("unexpected uv operation: {other:?}")),
        }
        Ok(())
    }
}

impl ExecutableUv {
    fn write_python_launcher(&self, current_dir: &Path) -> Result<(), String> {
        let python = current_dir.join(".venv/bin/python");
        fs::create_dir_all(python.parent().unwrap()).map_err(|error| error.to_string())?;
        let mut script = String::from("#!/bin/sh\n");
        if let Some(path) = &self.python_path {
            script.push_str(&format!(
                "export PYTHONPATH=\"{}${{PYTHONPATH:+:$PYTHONPATH}}\"\n",
                path.display()
            ));
        }
        script.push_str("exec python3 \"$@\"\n");
        fs::write(&python, script).map_err(|error| error.to_string())?;
        let mut permissions = fs::metadata(&python)
            .map_err(|error| error.to_string())?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&python, permissions).map_err(|error| error.to_string())
    }
}

struct PreparedFixture {
    _temporary: TempDir,
    providers: PathBuf,
    spec: PythonEnvironmentSpec,
}

impl PreparedFixture {
    fn new() -> Self {
        let temporary = tempdir().unwrap();
        let providers = temporary.path().join("providers");
        fs::create_dir(&providers).unwrap();
        let wheel = temporary.path().join("soma_provider.whl");
        fs::write(&wheel, b"sdk wheel").unwrap();
        let spec = PythonEnvironmentSpec {
            cache_root: temporary.path().join("cache"),
            runtime: PythonRuntimeFingerprint::new("cpython", "3.12.4", "linux-x86_64").unwrap(),
            python_executable: PathBuf::from("python3"),
            sdk_wheel: wheel,
            sdk_wheel_sha256: sha256_hex(b"sdk wheel"),
            uv_version: "0.11.31".to_owned(),
            offline: false,
        };
        Self {
            _temporary: temporary,
            providers,
            spec,
        }
    }

    fn source(&self, runner: ExecutableUv) -> FileProviderSource {
        let lifecycle = PythonEnvironmentLifecycle::with_runner("uv", self.spec.clone(), runner);
        FileProviderSource::new(&self.providers)
            .with_python_environment_preparer(Arc::new(lifecycle))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn provider<'a>(providers: &'a [Arc<dyn Provider>], name: &str) -> &'a Arc<dyn Provider> {
    providers
        .iter()
        .find(|provider| provider.catalog().provider.name == name)
        .unwrap()
}

fn provider_call(provider: &str, action: &str, params: serde_json::Value) -> ProviderCall {
    ProviderCall {
        provider: provider.to_owned(),
        action: action.to_owned(),
        params,
        principal: ProviderPrincipal::anonymous(),
        auth_mode: ProviderAuthMode::TrustedGateway,
        surface: ProviderSurface::Mcp,
        destructive_confirmed: false,
        limits: ProviderRequestLimits::default(),
        snapshot_id: "prepared-environment-test".to_owned(),
    }
}

#[tokio::test]
async fn zero_dependency_provider_activates_and_executes_from_prepared_environment() {
    let fixture = PreparedFixture::new();
    fs::write(
        fixture.providers.join("zero.py"),
        r#"PROVIDER = {"name": "prepared-zero", "kind": "python"}

def zero_echo(value: str):
    return {"value": value, "source": "prepared"}
"#,
    )
    .unwrap();
    let runner = ExecutableUv::default();

    let providers = fixture.source(runner.clone()).load().unwrap();
    let output = provider(&providers, "prepared-zero")
        .call(provider_call(
            "prepared-zero",
            "zero_echo",
            json!({"value": "hello"}),
        ))
        .await
        .unwrap()
        .into_value();

    assert_eq!(output, json!({"value": "hello", "source": "prepared"}));
    assert_eq!(runner.operations(), ["lock", "sync", "pip"]);
    let project = runner.project();
    let compact: String = project
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    assert!(compact.contains("dependencies=[]"), "{project}");
}

#[tokio::test]
async fn pep723_third_party_provider_activates_and_executes_from_prepared_environment() {
    let fixture = PreparedFixture::new();
    let site_packages = fixture._temporary.path().join("site-packages");
    fs::create_dir(&site_packages).unwrap();
    fs::write(
        site_packages.join("third_party_value.py"),
        "def decorate(value):\n    return f'dependency:{value}'\n",
    )
    .unwrap();
    fs::write(
        fixture.providers.join("third_party.py"),
        r#"# /// script
# requires-python = ">=3.11"
# dependencies = ["third-party-value>=1"]
# ///
from third_party_value import decorate

PROVIDER = {"name": "prepared-third-party", "kind": "python"}

def dependency_echo(value: str):
    return {"value": decorate(value)}
"#,
    )
    .unwrap();
    let runner = ExecutableUv::with_python_path(site_packages);

    let providers = fixture.source(runner.clone()).load().unwrap();
    let output = provider(&providers, "prepared-third-party")
        .call(provider_call(
            "prepared-third-party",
            "dependency_echo",
            json!({"value": "hello"}),
        ))
        .await
        .unwrap()
        .into_value();

    assert_eq!(output, json!({"value": "dependency:hello"}));
    let project = runner.project();
    assert!(project.contains("third-party-value>=1"), "{project}");
    assert!(
        project.contains("requires-python = \">=3.11\""),
        "{project}"
    );
}
