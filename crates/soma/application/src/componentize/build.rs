use std::{
    fs,
    io::{BufRead, BufReader, Read},
    path::{Component, Path, PathBuf},
    process::{Command, ExitStatus},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::{
    ComponentizeArtifact, ComponentizeState, WorkspaceLock, atomic_write, componentize_program,
    digest, digest_file, graduation_state, load_valid_state, read_bounded, unix_ms,
    verify_componentize_version, write_state,
};
use crate::graduation::{build_component_locked, ensure_no_transaction};

const MAX_COMPONENT_BYTES: usize = 64 * 1024 * 1024;
const BUILD_TIMEOUT_SECS: u64 = 120;
const VERIFY_TIMEOUT_SECS: u64 = 600;

pub(super) fn build(workspace: &Path, provider_root: &Path) -> anyhow::Result<Value> {
    let _lock = WorkspaceLock::acquire(workspace)?;
    ensure_no_transaction(workspace)?;
    let graduation = graduation_state(workspace, provider_root)?;
    let mut state = load_valid_state(workspace, &graduation, true)?;
    let program = componentize_program()?;
    verify_componentize_version(&program)?;

    let staging = tempfile::Builder::new()
        .prefix(".componentize-build-")
        .tempdir_in(workspace)?;
    prepare_build_tree(staging.path(), &state)?;
    let status = run_isolated_build(staging.path(), &program)?;
    if !status.success() {
        anyhow::bail!("componentize-py isolated build failed with status {status}");
    }
    let output = staging.path().join("out.wasm");
    let bytes = mark_generated_component(&output)?;
    verify_generated_component(&output).map_err(anyhow::Error::msg)?;
    let component_sha256 = digest(&bytes);
    let component = workspace
        .join("componentize")
        .join(format!("component-{component_sha256}.wasm"));
    if let Ok(existing) = fs::read(&component)
        && existing != bytes
    {
        anyhow::bail!("componentize digest path already contains different bytes");
    }
    if !component.exists() {
        atomic_write(&component, &bytes)?;
        let mut permissions = fs::metadata(&component)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&component, permissions)?;
    }
    state.component = Some(ComponentizeArtifact {
        path: component.clone(),
        sha256: component_sha256.clone(),
    });
    state.verified = true;
    state.verified_unix_ms = Some(unix_ms());

    let candidate = build_component_locked(workspace, Some(&component), provider_root)?;
    let candidate_path = candidate
        .get("candidate")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("graduation candidate response omitted its path"))?;
    let candidate_sha256 = candidate
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("graduation candidate response omitted its digest"))?
        .to_owned();
    state.graduation_candidate = Some(ComponentizeArtifact {
        path: candidate_path.clone(),
        sha256: candidate_sha256.clone(),
    });
    write_state(workspace, &state)?;
    Ok(json!({
        "ok": true,
        "component": component,
        "component_sha256": component_sha256,
        "candidate": candidate_path,
        "candidate_sha256": candidate_sha256,
        "verified_under_soma_wasmtime": true,
        "comparison_required_before_activation": true,
    }))
}

pub(super) fn validate(workspace: &Path, provider_root: &Path) -> anyhow::Result<Value> {
    let _lock = WorkspaceLock::acquire(workspace)?;
    ensure_no_transaction(workspace)?;
    let graduation = graduation_state(workspace, provider_root)?;
    let mut state = load_valid_state(workspace, &graduation, true)?;
    let component = state
        .component
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no componentize artifact has been built"))?;
    if digest_file(&component.path, MAX_COMPONENT_BYTES)? != component.sha256 {
        anyhow::bail!("componentize artifact digest mismatch");
    }
    verify_generated_component(&component.path).map_err(anyhow::Error::msg)?;
    state.verified = true;
    state.verified_unix_ms = Some(unix_ms());
    write_state(workspace, &state)?;
    Ok(json!({
        "ok": true,
        "component": component.path,
        "sha256": component.sha256,
        "wit": "soma:provider@1.0.0",
        "verified_under_soma_wasmtime": true,
    }))
}

fn mark_generated_component(path: &Path) -> anyhow::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        anyhow::bail!("componentize output is not a regular file");
    }
    let mut bytes = read_bounded(path, MAX_COMPONENT_BYTES, "componentized artifact")?;
    soma_provider_adapters::wasm::mark_componentize_artifact(&mut bytes)
        .map_err(anyhow::Error::msg)?;
    fs::write(path, &bytes)?;
    Ok(bytes)
}

fn verify_generated_component(path: &Path) -> Result<(), String> {
    soma_provider_adapters::wasm::verify_component_artifact_before(
        path,
        Instant::now() + Duration::from_secs(VERIFY_TIMEOUT_SECS),
    )
}

fn prepare_build_tree(root: &Path, state: &ComponentizeState) -> anyhow::Result<()> {
    let app = root.join("app");
    let sdk = app.join("soma_provider");
    let dependencies = root.join("dependencies");
    fs::create_dir_all(&sdk)?;
    fs::create_dir_all(&dependencies)?;
    fs::copy(&state.source, app.join("provider_impl.py"))?;
    fs::write(
        app.join("soma_component_app.py"),
        include_str!("../../templates/componentize/app.py"),
    )?;
    fs::write(
        sdk.join("__init__.py"),
        include_str!("../../../../../packages/python/python/soma_provider/__init__.py"),
    )?;
    fs::write(
        sdk.join("_runtime.py"),
        include_str!("../../../../../packages/python/python/soma_provider/_runtime.py"),
    )?;
    fs::write(
        sdk.join("_componentize.py"),
        include_str!("../../../../../packages/python/python/soma_provider/_componentize.py"),
    )?;
    fs::write(
        sdk.join("models.py"),
        include_str!("../../../../../packages/python/python/soma_provider/models.py"),
    )?;
    fs::write(
        root.join("world.wit"),
        include_str!("../../../../../wit/soma-provider/world.wit"),
    )?;
    extract_wheels(root, &dependencies, state)?;
    Ok(())
}

fn extract_wheels(
    root: &Path,
    destination: &Path,
    state: &ComponentizeState,
) -> anyhow::Result<()> {
    if state.wheels.is_empty() {
        return Ok(());
    }
    let script = root.join("extract_wheels.py");
    fs::write(
        &script,
        r#"import os,stat,sys,zipfile
MAX_ENTRIES=10000
MAX_ENTRY_BYTES=64*1024*1024
MAX_EXPANDED_BYTES=256*1024*1024
root=os.path.realpath(sys.argv[1])
os.makedirs(root,exist_ok=True)
for wheel in sys.argv[2:]:
    with zipfile.ZipFile(wheel) as archive:
        infos=archive.infolist()
        names=[info.filename for info in infos]
        if len(infos) > MAX_ENTRIES:
            raise RuntimeError(f'wheel entry limit exceeded: {wheel}')
        if len(set(names)) != len(names):
            raise RuntimeError(f'wheel contains duplicate paths: {wheel}')
        if sum(info.file_size for info in infos) > MAX_EXPANDED_BYTES:
            raise RuntimeError(f'wheel expanded-size limit exceeded: {wheel}')
        for info in infos:
            name=info.filename
            if info.file_size > MAX_ENTRY_BYTES:
                raise RuntimeError(f'wheel entry too large: {name}')
            if info.flag_bits & 0x1:
                raise RuntimeError(f'encrypted wheel entry is unsupported: {name}')
            parts=name.split('/')
            if name.startswith('/') or any(part in ('','..') for part in parts if part != '') or chr(0) in name:
                raise RuntimeError(f'unsafe wheel path: {name}')
            mode=(info.external_attr >> 16) & 0o170000
            if stat.S_ISLNK(mode):
                raise RuntimeError(f'wheel symlink is unsupported: {name}')
            target=os.path.realpath(os.path.join(root,*[part for part in parts if part]))
            if target != root and not target.startswith(root + os.sep):
                raise RuntimeError(f'wheel path escapes destination: {name}')
            if info.is_dir():
                os.makedirs(target,exist_ok=True)
                continue
            os.makedirs(os.path.dirname(target),exist_ok=True)
            if os.path.exists(target):
                raise RuntimeError(f'wheel file collision: {name}')
            with archive.open(info) as source, open(target,'xb') as output:
                while True:
                    chunk=source.read(65536)
                    if not chunk: break
                    output.write(chunk)
"#,
    )?;
    let python = std::env::var_os("SOMA_COMPONENTIZE_PYTHON").unwrap_or_else(|| "python3".into());
    let mut command = Command::new(python);
    command
        .env_clear()
        .env("HOME", root)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONNOUSERSITE", "1")
        .args(["-I", script.to_string_lossy().as_ref()])
        .arg(destination);
    for wheel in &state.wheels {
        if digest_file(&wheel.path, 64 * 1024 * 1024)? != wheel.sha256 {
            anyhow::bail!("componentize wheel changed before extraction");
        }
        command.arg(&wheel.path);
    }
    let output = command.output()?;
    if !output.status.success() {
        anyhow::bail!(
            "componentize wheel extraction failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(1024)
                .collect::<String>()
        );
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn run_isolated_build(root: &Path, program: &Path) -> anyhow::Result<ExitStatus> {
    for required in ["/usr/bin/bwrap", "/usr/bin/prlimit", "/usr/bin/timeout"] {
        if !Path::new(required).is_file() {
            anyhow::bail!("componentize build requires {required}");
        }
    }
    let program = program.canonicalize()?;
    let program_root = program
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow::anyhow!("componentize-py installation root is invalid"))?;
    let mut command = Command::new("/usr/bin/timeout");
    command
        .env_clear()
        .args([
            "--signal=KILL",
            &format!("{BUILD_TIMEOUT_SECS}s"),
            "/usr/bin/bwrap",
        ])
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--clearenv",
            "--tmpfs",
            "/",
            "--dir",
            "/usr",
            "--ro-bind",
            "/usr",
            "/usr",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--dir",
            "/work",
            "--bind",
        ])
        .arg(root)
        .arg("/work");
    for path in ["/lib", "/lib64", "/bin"] {
        if Path::new(path).exists() {
            command.args(["--dir", path, "--ro-bind", path, path]);
        }
    }
    for path in ["/etc/ld.so.cache", "/etc/localtime"] {
        if Path::new(path).is_file() {
            if let Some(parent) = Path::new(path).parent() {
                append_dir_chain(&mut command, parent)?;
            }
            command.args(["--ro-bind", path, path]);
        }
    }
    let mut runtime_roots = vec![program_root.to_owned()];
    if let Some(interpreter_root) = shebang_runtime_root(&program)? {
        runtime_roots.push(interpreter_root);
    }
    runtime_roots.sort();
    runtime_roots.dedup();
    for runtime_root in runtime_roots {
        if !runtime_root.starts_with("/usr") {
            append_dir_chain(&mut command, &runtime_root)?;
            command
                .arg("--ro-bind")
                .arg(&runtime_root)
                .arg(&runtime_root);
        }
    }
    let nproc_limit = format!("--nproc={}", sandbox_nproc_limit()?);
    command
        .args([
            "--chdir",
            "/work/app",
            "--setenv",
            "HOME",
            "/tmp",
            "--setenv",
            "PYTHONDONTWRITEBYTECODE",
            "1",
            "--setenv",
            "PYTHONNOUSERSITE",
            "1",
            "--setenv",
            "RAYON_NUM_THREADS",
            "4",
            "/usr/bin/prlimit",
            "--as=8589934592",
            "--cpu=120",
        ])
        .arg(nproc_limit)
        .args(["--nofile=256", "--core=0", "--fsize=67108864", "--"])
        .arg(&program)
        .args([
            "-d",
            "/work/world.wit",
            "-w",
            "provider",
            "--world-module",
            "soma_wit",
            "componentize",
            "-p",
            "/work/app",
            "-p",
            "/work/dependencies",
            "soma_component_app",
            "-o",
            "/work/out.wasm",
        ]);
    Ok(command.status()?)
}

fn sandbox_nproc_limit() -> anyhow::Result<u64> {
    const SANDBOX_TASK_ALLOWANCE: u64 = 128;
    let self_status = fs::read_to_string("/proc/self/status")?;
    let uid = status_number(&self_status, "Uid:")?;
    let mut threads = 0u64;
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        let Ok(status) = fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        if status_number(&status, "Uid:").ok() == Some(uid) {
            threads = threads.saturating_add(status_number(&status, "Threads:").unwrap_or(1));
        }
    }
    Ok(threads.saturating_add(SANDBOX_TASK_ALLOWANCE).max(256))
}

fn status_number(status: &str, label: &str) -> anyhow::Result<u64> {
    status
        .lines()
        .find_map(|line| {
            line.strip_prefix(label)
                .and_then(|value| value.split_whitespace().next())
        })
        .ok_or_else(|| anyhow::anyhow!("{label} is missing from proc status"))?
        .parse()
        .map_err(anyhow::Error::from)
}

fn shebang_runtime_root(program: &Path) -> anyhow::Result<Option<PathBuf>> {
    let file = fs::File::open(program)?;
    let mut line = String::new();
    BufReader::new(file).take(4097).read_line(&mut line)?;
    if line.len() > 4096 {
        anyhow::bail!("componentize-py launcher shebang exceeds 4096 bytes");
    }
    let Some(shebang) = line.strip_prefix("#!") else {
        return Ok(None);
    };
    let interpreter = shebang
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("componentize-py launcher has an empty shebang"))?;
    let interpreter = Path::new(interpreter);
    if !interpreter.is_absolute() {
        anyhow::bail!("componentize-py launcher interpreter must be absolute");
    }
    let interpreter = interpreter.canonicalize()?;
    let root = interpreter
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow::anyhow!("componentize-py interpreter root is invalid"))?;
    Ok((!root.starts_with("/usr")).then(|| root.to_owned()))
}

#[cfg(not(target_os = "linux"))]
fn run_isolated_build(_root: &Path, _program: &Path) -> anyhow::Result<ExitStatus> {
    anyhow::bail!("componentize builds require the enforced Linux namespace boundary")
}

fn append_dir_chain(command: &mut Command, path: &Path) -> anyhow::Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => current.push("/"),
            Component::Normal(value) => {
                current.push(value);
                if current != Path::new("/") {
                    command.arg("--dir").arg(&current);
                }
            }
            _ => anyhow::bail!("componentize program path is not a canonical absolute path"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_componentize_build_validates_under_soma_wasmtime() {
        if std::env::var_os("SOMA_COMPONENTIZE_E2E").as_deref() != Some(std::ffi::OsStr::new("1")) {
            return;
        }
        let program = super::super::componentize_program().expect("componentize-py program");
        super::super::verify_componentize_version(&program).expect("exact componentize-py");
        let root = tempfile::tempdir().expect("build root");
        let app = root.path().join("app");
        let sdk = app.join("soma_provider");
        fs::create_dir_all(&sdk).expect("SDK directory");
        fs::create_dir(root.path().join("dependencies")).expect("dependency directory");
        fs::write(
            app.join("provider_impl.py"),
            "from soma_provider import tool\n\n@tool\ndef echo(value: int) -> dict[str, int]:\n    return {'value': value}\n",
        )
        .expect("provider");
        fs::write(
            app.join("soma_component_app.py"),
            include_str!("../../templates/componentize/app.py"),
        )
        .expect("adapter");
        fs::write(
            sdk.join("__init__.py"),
            include_str!("../../../../../packages/python/python/soma_provider/__init__.py"),
        )
        .expect("SDK");
        fs::write(
            sdk.join("_runtime.py"),
            include_str!("../../../../../packages/python/python/soma_provider/_runtime.py"),
        )
        .expect("runtime");
        fs::write(
            sdk.join("_componentize.py"),
            include_str!("../../../../../packages/python/python/soma_provider/_componentize.py"),
        )
        .expect("scanner");
        fs::write(
            sdk.join("models.py"),
            include_str!("../../../../../packages/python/python/soma_provider/models.py"),
        )
        .expect("models");
        fs::write(
            root.path().join("world.wit"),
            include_str!("../../../../../wit/soma-provider/world.wit"),
        )
        .expect("WIT");

        let status = run_isolated_build(root.path(), &program).expect("isolated build");
        assert!(status.success(), "componentize-py failed with {status}");
        let artifact = root.path().join("out.wasm");
        mark_generated_component(&artifact).expect("componentize marker");
        if let Some(destination) = std::env::var_os("SOMA_COMPONENTIZE_E2E_ARTIFACT") {
            fs::copy(&artifact, PathBuf::from(destination)).expect("persist E2E artifact");
        }
        verify_generated_component(&artifact).expect("Soma Wasmtime verification");
        let output = soma_provider_adapters::wasm::invoke_authorized_component_artifact(
            &artifact,
            &json!({"action": "echo", "arguments": {"value": 7}}),
            &soma_provider_core::HostCapabilities::default(),
            &soma_provider_core::ProviderInvocationContext {
                request_id: "componentize-e2e".to_owned(),
                actor_id: Some("soma-verifier".to_owned()),
                actor_scopes: vec!["soma:read".to_owned()],
                ..Default::default()
            },
        )
        .expect("Soma component invocation");
        assert_eq!(output, json!({"value": 7}));
    }
}
