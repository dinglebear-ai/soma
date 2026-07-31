//! Fail-closed OS containment for brokered Python workers.

#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};

use tokio::process::Command;

use super::{host::PythonExecutionProfile, supervisor::PythonSupervisorError};

#[derive(Debug, Default)]
pub(super) struct CgroupGuard {
    #[cfg(target_os = "linux")]
    path: Option<PathBuf>,
}

impl CgroupGuard {
    #[cfg(target_os = "linux")]
    pub(super) fn attach(
        pid: Option<u32>,
        profile: PythonExecutionProfile,
    ) -> Result<Self, PythonSupervisorError> {
        if profile != PythonExecutionProfile::Brokered {
            return Ok(Self::default());
        }
        let pid = pid.ok_or_else(|| containment_unavailable("missing child PID"))?;
        let root = std::env::var_os("SOMA_PYTHON_BROKER_CGROUP_ROOT")
            .map(PathBuf::from)
            .ok_or_else(|| containment_unavailable("cgroup root is not configured"))?;
        if !root.is_absolute() {
            return Err(containment_unavailable("cgroup root is not absolute"));
        }
        let filesystem = nix::sys::statfs::statfs(&root)
            .map_err(|_| containment_unavailable("cgroup root is inaccessible"))?;
        if filesystem.filesystem_type() != nix::sys::statfs::CGROUP2_SUPER_MAGIC {
            return Err(containment_unavailable("cgroup root is not cgroup v2"));
        }
        let path = root.join(format!("soma-python-{pid}"));
        std::fs::create_dir(&path)
            .map_err(|_| containment_unavailable("cgroup child creation failed"))?;
        let configure = || -> std::io::Result<()> {
            std::fs::write(path.join("memory.max"), b"268435456")?;
            std::fs::write(path.join("pids.max"), b"64")?;
            std::fs::write(path.join("cpu.max"), b"50000 100000")?;
            std::fs::write(path.join("cgroup.procs"), pid.to_string())?;
            Ok(())
        };
        if configure().is_err() {
            let _ = std::fs::remove_dir(&path);
            return Err(containment_unavailable(
                "cgroup limits or attachment failed",
            ));
        }
        Ok(Self { path: Some(path) })
    }

    #[cfg(not(target_os = "linux"))]
    pub(super) fn attach(
        _pid: Option<u32>,
        profile: PythonExecutionProfile,
    ) -> Result<Self, PythonSupervisorError> {
        if profile == PythonExecutionProfile::Brokered {
            return Err(containment_unavailable("brokered mode is unsupported"));
        }
        Ok(Self::default())
    }
}

#[cfg(target_os = "linux")]
impl Drop for CgroupGuard {
    fn drop(&mut self) {
        let Some(path) = self.path.take() else {
            return;
        };
        let _ = std::fs::write(path.join("cgroup.kill"), b"1");
        let _ = std::fs::remove_dir(path);
    }
}

#[cfg(target_os = "linux")]
pub(super) struct BrokeredLaunch {
    listener: tokio::net::UnixListener,
    _directory: tempfile::TempDir,
    seccomp: std::os::fd::OwnedFd,
}

#[cfg(target_os = "linux")]
impl BrokeredLaunch {
    pub(super) fn prepare(
        python: &str,
        provider: &Path,
    ) -> Result<(Command, Self, PathBuf), PythonSupervisorError> {
        use std::{
            io::{Seek, Write},
            os::fd::{AsFd, AsRawFd},
        };

        use libseccomp::{
            ScmpAction, ScmpArgCompare, ScmpCompareOp, ScmpFilterContext, ScmpSyscall,
        };
        use nix::{
            fcntl::{FcntlArg, FdFlag, fcntl},
            sys::memfd::{MFdFlags, memfd_create},
        };

        if !Path::new("/usr/bin/bwrap").is_file() || !Path::new("/usr/bin/prlimit").is_file() {
            return Err(containment_unavailable(
                "bubblewrap or prlimit is unavailable",
            ));
        }
        let directory = tempfile::Builder::new()
            .prefix("soma-python-broker-")
            .tempdir()
            .map_err(|_| containment_unavailable("control directory creation failed"))?;
        let socket_path = directory.path().join("control.sock");
        let listener = tokio::net::UnixListener::bind(&socket_path)
            .map_err(|_| containment_unavailable("control socket bind failed"))?;
        let seccomp = memfd_create("soma-python-seccomp", MFdFlags::empty())
            .map_err(|_| containment_unavailable("seccomp memfd creation failed"))?;
        let mut seccomp_file = std::fs::File::from(seccomp);
        let mut filter = ScmpFilterContext::new(ScmpAction::Allow)
            .map_err(|_| containment_unavailable("seccomp initialization failed"))?;
        for syscall in [
            "mount",
            "umount2",
            "pivot_root",
            "ptrace",
            "bpf",
            "perf_event_open",
            "keyctl",
            "add_key",
            "request_key",
            "userfaultfd",
        ] {
            let syscall = ScmpSyscall::from_name(syscall)
                .map_err(|_| containment_unavailable("seccomp syscall resolution failed"))?;
            filter
                .add_rule(ScmpAction::Errno(nix::libc::EPERM), syscall)
                .map_err(|_| containment_unavailable("seccomp rule installation failed"))?;
        }
        let socket = ScmpSyscall::from_name("socket")
            .map_err(|_| containment_unavailable("seccomp socket resolution failed"))?;
        for family in [nix::libc::AF_INET, nix::libc::AF_INET6] {
            filter
                .add_rule_conditional(
                    ScmpAction::Errno(nix::libc::EPERM),
                    socket,
                    &[ScmpArgCompare::new(0, ScmpCompareOp::Equal, family as u64)],
                )
                .map_err(|_| containment_unavailable("seccomp network rule failed"))?;
        }
        filter
            .export_bpf(&seccomp_file)
            .map_err(|_| containment_unavailable("seccomp export failed"))?;
        seccomp_file
            .flush()
            .map_err(|_| containment_unavailable("seccomp flush failed"))?;
        seccomp_file
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|_| containment_unavailable("seccomp rewind failed"))?;
        let seccomp: std::os::fd::OwnedFd = seccomp_file.into();
        fcntl(seccomp.as_fd(), FcntlArg::F_SETFD(FdFlag::empty()))
            .map_err(|_| containment_unavailable("seccomp descriptor inheritance failed"))?;

        let provider = provider
            .canonicalize()
            .map_err(|_| containment_unavailable("provider source is inaccessible"))?;
        if !provider.is_file() {
            return Err(containment_unavailable(
                "provider source is not a regular file",
            ));
        }
        let sandbox_provider = PathBuf::from("/provider").join(
            provider
                .file_name()
                .ok_or_else(|| containment_unavailable("provider source has no file name"))?,
        );
        let python = Path::new(python)
            .canonicalize()
            .map_err(|_| containment_unavailable("Python interpreter is inaccessible"))?;
        let python_under_usr = python.starts_with("/usr");
        let (sandbox_python, python_root) = if python_under_usr {
            (python.clone(), None)
        } else {
            let root = python
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| containment_unavailable("Python environment root is invalid"))?;
            let relative = python
                .strip_prefix(root)
                .map_err(|_| containment_unavailable("Python environment path is invalid"))?;
            (PathBuf::from("/python-env").join(relative), Some(root))
        };

        let seccomp_fd = seccomp.as_raw_fd();
        let mut command = Command::new("/usr/bin/prlimit");
        command.args([
            "--as=536870912",
            "--nofile=128",
            "--core=0",
            "--fsize=16777216",
            "--",
            "/usr/bin/bwrap",
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--hostname",
            "soma-python",
            "--tmpfs",
            "/",
            "--dir",
            "/usr",
            "--ro-bind",
            "/usr",
            "/usr",
            "--dir",
            "/provider",
            "--ro-bind",
            provider
                .to_str()
                .ok_or_else(|| containment_unavailable("provider path is not UTF-8"))?,
            sandbox_provider
                .to_str()
                .ok_or_else(|| containment_unavailable("sandbox provider path is not UTF-8"))?,
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--tmpfs",
            "/run",
            "--ro-bind",
            directory
                .path()
                .to_str()
                .ok_or_else(|| containment_unavailable("control path is not UTF-8"))?,
            "/run/soma",
            "--seccomp",
            &seccomp_fd.to_string(),
        ]);
        for root in ["/lib", "/lib64", "/bin"] {
            if Path::new(root).exists() {
                command.args(["--dir", root, "--ro-bind", root, root]);
            }
        }
        for source in [
            "/etc/ssl",
            "/etc/pki",
            "/etc/ca-certificates",
            "/etc/alternatives",
            "/etc/ld.so.cache",
            "/etc/resolv.conf",
            "/etc/hosts",
            "/etc/nsswitch.conf",
            "/etc/localtime",
        ] {
            let source_path = Path::new(source);
            if source_path.exists() {
                if let Some(parent) = source_path.parent() {
                    command.args([
                        "--dir",
                        parent
                            .to_str()
                            .ok_or_else(|| containment_unavailable("system path is not UTF-8"))?,
                    ]);
                }
                command.args(["--ro-bind", source, source]);
            }
        }
        if let Some(root) = python_root {
            command.args([
                "--dir",
                "/python-env",
                "--ro-bind",
                root.to_str()
                    .ok_or_else(|| containment_unavailable("Python path is not UTF-8"))?,
                "/python-env",
            ]);
        }
        command.args([
            "--chdir",
            "/provider",
            "--setenv",
            "HOME",
            "/tmp",
            "--",
            sandbox_python
                .to_str()
                .ok_or_else(|| containment_unavailable("sandbox Python path is not UTF-8"))?,
            "-I",
            "-m",
            "soma_provider.runner",
        ]);
        Ok((
            command,
            Self {
                listener,
                _directory: directory,
                seccomp,
            },
            sandbox_provider,
        ))
    }

    pub(super) fn retain_until_spawn(&self) {
        let _ = &self.seccomp;
    }

    pub(super) async fn accept(&self) -> Result<tokio::net::UnixStream, PythonSupervisorError> {
        self.listener
            .accept()
            .await
            .map(|(stream, _)| stream)
            .map_err(|_| containment_unavailable("control socket accept failed"))
    }
}

#[cfg(not(target_os = "linux"))]
pub(super) struct BrokeredLaunch;

#[cfg(not(target_os = "linux"))]
impl BrokeredLaunch {
    pub(super) fn prepare(
        _python: &str,
        _provider: &std::path::Path,
    ) -> Result<(Command, Self, std::path::PathBuf), PythonSupervisorError> {
        Err(containment_unavailable("brokered mode is unsupported"))
    }

    pub(super) fn retain_until_spawn(&self) {}
}

fn containment_unavailable(stage: &'static str) -> PythonSupervisorError {
    PythonSupervisorError::new(
        "python_brokered_containment_unavailable",
        format!("Brokered Python containment is unavailable ({stage}); execution failed closed"),
    )
}
