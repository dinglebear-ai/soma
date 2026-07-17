//! Behavior-freeze coverage for `soma doctor` — the one CLI command that has a
//! human/coloured-text rendering path (`print_doctor_report` in
//! `crates/soma/cli/src/doctor.rs`).
//!
//! Before this test, `print_doctor_report` was never invoked by any test:
//! `doctor_tests.rs` / `doctor/checks_tests.rs` only unit-test individual
//! `check_*` functions, never `run_doctor()`'s composed output, and no test
//! spawned `soma doctor` as a subprocess. `doctor.rs` imports
//! `soma_config::{default_data_dir, Config}` directly, so a field
//! rename or default-value change made while extracting `soma-config` (PR 13)
//! would silently alter both the human report and the `--json` payload with
//! nothing failing.
//!
//! This spawns the real `soma` binary (mirrors `provider_cli.rs`'s pattern)
//! with a fully deterministic environment — an isolated data dir, a
//! reserved-then-released loopback port, and `PATH` pointing only at the
//! directory containing the test binary (so the "binary in PATH" check is
//! deterministic too) — and pins both the `--json` payload and the exact
//! human-readable report text.

use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::Value;
use tempfile::tempdir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_soma")
}

fn bin_dir() -> PathBuf {
    Path::new(binary())
        .parent()
        .expect("test binary path should have a parent directory")
        .to_path_buf()
}

fn unused_loopback_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
    // listener drops here, freeing the port for the subprocess to check.
}

struct DoctorEnv {
    _home: tempfile::TempDir,
    data_dir: tempfile::TempDir,
    port: u16,
}

impl DoctorEnv {
    fn new() -> Self {
        Self {
            _home: tempdir().expect("home tempdir"),
            data_dir: tempdir().expect("data tempdir"),
            port: unused_loopback_port(),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(binary());
        command
            .env("HOME", self._home.path())
            .env("SOMA_HOME", self.data_dir.path())
            .env("SOMA_API_URL", "")
            .env("SOMA_API_KEY", "")
            .env("SOMA_MCP_PORT", self.port.to_string())
            .env("PATH", bin_dir())
            .env_remove("SOMA_MCP_HOST")
            .env_remove("SOMA_MCP_TOKEN")
            .env_remove("SOMA_MCP_NO_AUTH")
            .env_remove("SOMA_NOAUTH")
            .env_remove("SOMA_MCP_AUTH_MODE")
            .env_remove("SOMA_MCP_GOOGLE_CLIENT_ID")
            .env_remove("SOMA_MCP_GOOGLE_CLIENT_SECRET")
            .env_remove("SOMA_MCP_ALLOWED_HOSTS")
            .env_remove("SOMA_MCP_ALLOWED_ORIGINS")
            .env_remove("SOMA_MCP_PUBLIC_URL")
            .env_remove("SOMA_PROVIDER_DIR")
            .env_remove("RUST_LOG");
        command
    }

    fn data_dir(&self) -> &Path {
        self.data_dir.path()
    }
}

fn pass_line(name: &str, value: &str) -> String {
    // Mirrors doctor.rs's `println!("  {}  {:<28}  {}{}", "✓", name, value, "")`
    // exactly (colour codes are identity here since stdout/stderr are piped,
    // not a terminal, so `print_doctor_report`'s `color` flag is false).
    format!("  ✓  {name:<28}  {value}")
}

fn fail_header_line(name: &str) -> String {
    format!("  ✗  {name}")
}

#[test]
fn doctor_json_reports_deterministic_checks_for_a_clean_loopback_environment() {
    let env = DoctorEnv::new();
    let output = env
        .command()
        .args(["doctor", "--json"])
        .output()
        .expect("run soma doctor --json");

    assert!(
        !output.status.success(),
        "doctor should exit non-zero when required credentials are missing"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("doctor found 2 issue(s)"),
        "stderr={stderr}"
    );

    let checks: Value = serde_json::from_slice(&output.stdout).expect("doctor --json output");
    let checks = checks.as_array().expect("doctor --json is a JSON array");
    assert_eq!(checks.len(), 8, "checks={checks:#?}");

    let data_dir = env.data_dir();
    let config_path = data_dir.join("config.toml");
    let bin_path = bin_dir().join("soma");

    assert_eq!(
        checks[0],
        serde_json::json!({
            "category": "config",
            "name": "Config file",
            "ok": true,
            "value": format!(
                "{} (not found — using env vars / defaults)",
                config_path.display()
            ),
        })
    );
    assert_eq!(
        checks[1],
        serde_json::json!({
            "category": "config",
            "name": format!("Data directory: {}", data_dir.display()),
            "ok": true,
            "value": "writable",
        })
    );
    assert_eq!(
        checks[2],
        serde_json::json!({
            "category": "config",
            "name": format!("Log directory: {}", data_dir.join("logs").display()),
            "ok": true,
            "value": "writable",
        })
    );
    assert_eq!(
        checks[3],
        serde_json::json!({
            "category": "config",
            "name": "Binary in PATH: soma",
            "ok": true,
            "value": bin_path.display().to_string(),
        })
    );
    assert_eq!(
        checks[4],
        serde_json::json!({
            "category": "credentials",
            "name": "SOMA_API_URL",
            "ok": false,
            "hint": "Not set.\n    \
                 → Add to ~/.soma/.env:  SOMA_API_URL=<your_value>\n    \
                 → Or export in your shell: export SOMA_API_URL=<your_value>\n    \
                 CUSTOMIZE: Replace ~/.soma/ with your service data dir.",
        })
    );
    assert_eq!(
        checks[5],
        serde_json::json!({
            "category": "credentials",
            "name": "SOMA_API_KEY",
            "ok": false,
            "hint": "Not set.\n    \
                 → Add to ~/.soma/.env:  SOMA_API_KEY=<your_value>\n    \
                 → Or export in your shell: export SOMA_API_KEY=<your_value>\n    \
                 CUSTOMIZE: Replace ~/.soma/ with your service data dir.",
        })
    );
    assert_eq!(
        checks[6],
        serde_json::json!({
            "category": "server",
            "name": format!("MCP bind 127.0.0.1:{}", env.port),
            "ok": true,
            "value": "available",
        })
    );
    assert_eq!(
        checks[7],
        serde_json::json!({
            "category": "auth",
            "name": "Auth mode",
            "ok": true,
            "value": "no-auth (loopback bind)",
        })
    );
}

#[test]
fn doctor_human_report_renders_the_same_checks_print_doctor_report_prints() {
    let env = DoctorEnv::new();

    // Pull the structured checks first so the exact per-line text we assert
    // against is derived independently of the human formatter under test.
    let json_output = env
        .command()
        .args(["doctor", "--json"])
        .output()
        .expect("run soma doctor --json");
    let checks: Value = serde_json::from_slice(&json_output.stdout).expect("doctor --json output");
    let checks = checks.as_array().expect("doctor --json is a JSON array");

    let human_output = env
        .command()
        .arg("doctor")
        .output()
        .expect("run soma doctor");
    assert!(!human_output.status.success());
    let stdout = String::from_utf8_lossy(&human_output.stdout);

    // Header (CARGO_PKG_VERSION of soma-cli is deliberately not pinned here —
    // only the surrounding structure is).
    assert!(stdout.contains("soma-mcp v"), "stdout={stdout}");
    assert!(stdout.contains("— environment check"), "stdout={stdout}");

    // Section headers appear, in the fixed §48 order, and the unused
    // "Connectivity" section (no upstream configured) is skipped entirely.
    let config_pos = stdout.find("\n  Config\n").expect("Config header");
    let creds_pos = stdout
        .find("\n  Service credentials\n")
        .expect("Service credentials header");
    let server_pos = stdout.find("\n  MCP server\n").expect("MCP server header");
    let auth_pos = stdout
        .find("\n  Authentication\n")
        .expect("Authentication header");
    assert!(config_pos < creds_pos);
    assert!(creds_pos < server_pos);
    assert!(server_pos < auth_pos);
    assert!(!stdout.contains("Connectivity"), "stdout={stdout}");

    // Every check's rendered line matches its JSON fields exactly.
    for check in checks {
        let name = check["name"].as_str().unwrap();
        if check["ok"].as_bool().unwrap() {
            let value = check["value"].as_str().unwrap();
            let line = pass_line(name, value);
            assert!(
                stdout.contains(&line),
                "expected line {line:?} in stdout={stdout}"
            );
        } else {
            let header = fail_header_line(name);
            assert!(
                stdout.contains(&header),
                "expected line {header:?} in stdout={stdout}"
            );
            let hint = check["hint"].as_str().unwrap();
            for hint_line in hint.lines() {
                let rendered = format!("    {hint_line}");
                assert!(
                    stdout.contains(&rendered),
                    "expected hint line {rendered:?} in stdout={stdout}"
                );
            }
        }
    }

    // Summary line.
    let issues = checks
        .iter()
        .filter(|c| !c["ok"].as_bool().unwrap())
        .count();
    let noun = if issues == 1 { "issue" } else { "issues" };
    let summary = format!("  ✗  {issues} {noun} found. Fix before running: soma serve");
    assert!(
        stdout.contains(&summary),
        "expected summary {summary:?} in stdout={stdout}"
    );
}
