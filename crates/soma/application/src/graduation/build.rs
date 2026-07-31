use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

/// Run untrusted Cargo build scripts and proc macros inside a fail-closed,
/// offline Linux namespace with bounded CPU, address space, and process count.
#[cfg(target_os = "linux")]
pub(super) fn run_isolated_component_build(workspace: &Path) -> anyhow::Result<ExitStatus> {
    let toolchain = PathBuf::from(
        String::from_utf8(
            Command::new("rustc")
                .args(["--print", "sysroot"])
                .output()?
                .stdout,
        )?
        .trim(),
    )
    .canonicalize()?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("HOME is unavailable for the isolated Cargo cache"))?;
    let registry = home.join(".cargo/registry").canonicalize()?;
    let git = home.join(".cargo/git").canonicalize()?;
    let workspace = workspace.canonicalize()?;
    let target = workspace.join("target");
    fs::create_dir_all(&target)?;
    let manifest = workspace.join("Cargo.toml");
    Command::new("/usr/bin/timeout")
        .env_clear()
        .args(["--signal=KILL", "30s", "/usr/bin/bwrap"])
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "--ro-bind",
            "/etc",
            "/etc",
            "--ro-bind",
            "/lib",
            "/lib",
            "--ro-bind",
            "/lib64",
            "/lib64",
            "--tmpfs",
            "/tmp",
            "--tmpfs",
            "/home",
            "--dir",
            "/cargo",
            "--ro-bind",
        ])
        .arg(registry)
        .arg("/cargo/registry")
        .arg("--ro-bind")
        .arg(git)
        .arg("/cargo/git")
        .arg("--ro-bind")
        .arg(toolchain)
        .arg("/toolchain")
        .arg("--ro-bind")
        .arg(&workspace)
        .arg(&workspace)
        .arg("--bind")
        .arg(&target)
        .arg(&target)
        .arg("--chdir")
        .arg(&workspace)
        .args([
            "--setenv",
            "HOME",
            "/home",
            "--setenv",
            "CARGO_HOME",
            "/cargo",
            "--setenv",
            "RUSTC",
            "/toolchain/bin/rustc",
            "--setenv",
            "PATH",
            "/toolchain/bin:/usr/bin:/bin",
            "/usr/bin/prlimit",
            "--as=2147483648",
            "--cpu=30",
            "--nproc=128",
            "--",
            "/toolchain/bin/cargo",
            "build",
            "--offline",
            "--locked",
            "--manifest-path",
        ])
        .arg(manifest)
        .args(["--target", "wasm32-wasip2", "--features", "component"])
        .status()
        .map_err(Into::into)
}

#[cfg(not(target_os = "linux"))]
pub(super) fn run_isolated_component_build(_workspace: &Path) -> anyhow::Result<ExitStatus> {
    anyhow::bail!("graduation builds require the enforced Linux containment boundary")
}
