//! Bounded live smoke for `SOMA_MCP_TRACE_HEADERS`.

use std::{
    io::Read,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

use crate::scripts_lane_a::AuthSmokeResults;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const TRACEPARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01";

pub fn test_trace_headers(_args: &[String]) -> Result<()> {
    let binary = build_soma_binary()?;
    let mut results = AuthSmokeResults::default();

    run_scenario(&binary, &mut results, "off", "off", |port| {
        off_mode_checks(port)
    })?;
    run_scenario(&binary, &mut results, "trusted", "trusted", |port| {
        trusted_mode_checks(port, false)
    })?;
    run_scenario(
        &binary,
        &mut results,
        "trusted-with-baggage",
        "trusted-with-baggage",
        |port| trusted_mode_checks(port, true),
    )?;

    println!("\n{} passed, {} failed", results.pass, results.fail);
    if results.fail == 0 {
        Ok(())
    } else {
        bail!("{} trace-header smoke check(s) failed", results.fail)
    }
}

fn build_soma_binary() -> Result<PathBuf> {
    println!("==> Building soma binary once...");
    let output = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "soma",
            "--features",
            "full",
            "--message-format=json",
        ])
        .output()
        .context("run cargo build")?;
    if !output.status.success() {
        bail!(
            "cargo build failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if message["reason"] == "compiler-artifact"
            && message["target"]["name"] == "soma"
            && message["executable"].is_string()
        {
            return Ok(PathBuf::from(
                message["executable"]
                    .as_str()
                    .context("executable path should be a string")?,
            ));
        }
    }
    bail!("cargo build did not report a soma binary artifact")
}

fn run_scenario(
    binary: &Path,
    results: &mut AuthSmokeResults,
    label: &str,
    trace_headers_env: &str,
    checks: impl FnOnce(u16) -> Result<Vec<(String, bool)>>,
) -> Result<()> {
    println!("==> Scenario: SOMA_MCP_TRACE_HEADERS={trace_headers_env}");
    let home = tempfile::tempdir().context("create isolated SOMA_HOME")?;
    let port = free_port()?;
    let mut server = ServerGuard::spawn(binary, home.path(), port, trace_headers_env)?;
    server.wait_for_health(port)?;

    for (check_label, passed) in checks(port)? {
        record(results, label, &check_label, passed);
    }
    let log = server.captured_log();
    assert_safe_log_fields(&log, results, label, trace_headers_env);
    Ok(())
}

fn record(results: &mut AuthSmokeResults, scenario: &str, label: &str, passed: bool) {
    let label = format!("{scenario}: {label}");
    if passed {
        results.pass(&label);
    } else {
        results.fail(&label);
    }
}

fn off_mode_checks(port: u16) -> Result<Vec<(String, bool)>> {
    let status = curl_status(port, &[("traceparent", TRACEPARENT)])?;
    let preflight = curl_preflight(port, "TraceParent")?;
    Ok(vec![
        (
            "status tool call succeeds with traceparent present".to_owned(),
            status == 200,
        ),
        (
            "CORS preflight denies traceparent".to_owned(),
            !preflight.to_ascii_lowercase().contains("traceparent"),
        ),
    ])
}

fn trusted_mode_checks(port: u16, baggage_enabled: bool) -> Result<Vec<(String, bool)>> {
    let mut checks = Vec::new();
    let status = curl_status(
        port,
        &[
            ("traceparent", TRACEPARENT),
            ("tracestate", "vendor=value"),
            ("baggage", "region=us-east-1"),
        ],
    )?;
    checks.push((
        "valid trace context tool call succeeds".to_owned(),
        status == 200,
    ));

    let duplicate = curl_status(
        port,
        &[
            ("traceparent", TRACEPARENT),
            (
                "traceparent",
                "00-11112222333344445555666677778888-1111222233334444-01",
            ),
        ],
    )?;
    checks.push((
        "duplicate traceparent does not crash the server".to_owned(),
        duplicate == 200,
    ));

    let non_ascii = curl_status(port, &[("traceparent", "00-\u{00e9}-invalid-01")])?;
    checks.push((
        "non-ASCII traceparent does not crash the server".to_owned(),
        non_ascii == 200,
    ));

    let preflight = curl_preflight(port, "TraceParent, TraceState, Baggage")?;
    let preflight = preflight.to_ascii_lowercase();
    checks.push((
        "CORS preflight allows traceparent".to_owned(),
        preflight.contains("traceparent"),
    ));
    checks.push((
        "CORS preflight allows tracestate".to_owned(),
        preflight.contains("tracestate"),
    ));
    checks.push((
        "baggage CORS allowance matches mode".to_owned(),
        preflight.contains("baggage") == baggage_enabled,
    ));
    Ok(checks)
}

fn assert_safe_log_fields(log: &str, results: &mut AuthSmokeResults, scenario: &str, mode: &str) {
    match mode {
        "off" => record(
            results,
            scenario,
            "logs show HTTP trace extraction disabled",
            log.contains("http_trace_headers_present=false"),
        ),
        "trusted" => {
            record(
                results,
                scenario,
                "logs contain the safe trace ID prefix",
                log.contains("trace_id_prefix=Some(\"0af76519\")"),
            );
            record(
                results,
                scenario,
                "logs prove baggage was stripped",
                log.contains("baggage_member_count=0"),
            );
        }
        "trusted-with-baggage" => {
            record(
                results,
                scenario,
                "logs contain the safe trace ID prefix",
                log.contains("trace_id_prefix=Some(\"0af76519\")"),
            );
            record(
                results,
                scenario,
                "logs contain only the safe baggage count",
                log.contains("baggage_member_count=1"),
            );
        }
        _ => unreachable!("known trace-header mode"),
    }
    record(
        results,
        scenario,
        "raw baggage value is absent from logs",
        !log.contains("region=us-east-1") && !log.contains("accessToken"),
    );
}

fn curl_status(port: u16, headers: &[(&str, &str)]) -> Result<u16> {
    let timeout = REQUEST_TIMEOUT.as_secs().to_string();
    let mut cmd = Command::new("curl");
    cmd.args([
        "-s",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "-X",
        "POST",
        "-H",
        "Content-Type: application/json",
        "-H",
        "Accept: application/json, text/event-stream",
        "--max-time",
        &timeout,
    ]);
    for (name, value) in headers {
        cmd.args(["-H", &format!("{name}: {value}")]);
    }
    cmd.args([
        "-d",
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"soma","arguments":{"action":"status"}}}"#,
        &format!("http://127.0.0.1:{port}/mcp"),
    ]);
    let output = cmd.output().context("run curl tool call")?;
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u16>()
        .context("parse curl status code")
}

fn curl_preflight(port: u16, requested_headers: &str) -> Result<String> {
    let output = curl_preflight_command(port, requested_headers)
        .output()
        .context("run curl preflight")?;
    checked_preflight_output(output)
}

fn checked_preflight_output(output: std::process::Output) -> Result<String> {
    if !output.status.success() {
        bail!(
            "curl preflight failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn curl_preflight_command(port: u16, requested_headers: &str) -> Command {
    let timeout = REQUEST_TIMEOUT.as_secs().to_string();
    let mut command = Command::new("curl");
    command.args([
        "-s",
        "-i",
        "--max-time",
        &timeout,
        "-X",
        "OPTIONS",
        "-H",
        &format!("Origin: http://127.0.0.1:{port}"),
        "-H",
        "Access-Control-Request-Method: POST",
        "-H",
        &format!("Access-Control-Request-Headers: {requested_headers}"),
        &format!("http://127.0.0.1:{port}/mcp"),
    ]);
    command
}

fn free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("bind ephemeral port")?;
    Ok(listener.local_addr()?.port())
}

struct ServerGuard {
    child: Child,
    log_path: PathBuf,
}

impl ServerGuard {
    fn spawn(binary: &Path, home: &Path, port: u16, trace_headers: &str) -> Result<Self> {
        let log_path = home.join("server.log");
        let log_file = std::fs::File::create(&log_path).context("create server log")?;
        let mut command = Command::new(binary);
        for (key, _) in
            std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("SOMA_"))
        {
            command.env_remove(key);
        }
        let child = command
            .arg("serve")
            .current_dir(home)
            .env("HOME", home)
            .env("SOMA_HOME", home)
            .env("SOMA_MCP_HOST", "127.0.0.1")
            .env("SOMA_MCP_PORT", port.to_string())
            .env("SOMA_MCP_NO_AUTH", "true")
            .env("SOMA_MCP_TRACE_HEADERS", trace_headers)
            .env("SOMA_RUNTIME_MODE", "local")
            .env("RUST_LOG", "soma_mcp=info,soma=info")
            .stdout(Stdio::from(
                log_file.try_clone().context("clone log handle")?,
            ))
            .stderr(Stdio::from(log_file))
            .spawn()
            .context("spawn soma serve")?;
        Ok(Self { child, log_path })
    }

    fn wait_for_health(&mut self, port: u16) -> Result<()> {
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().context("poll soma process")? {
                bail!("soma exited before becoming healthy: {status}");
            }
            if Instant::now() > deadline {
                bail!("server did not become healthy within {STARTUP_TIMEOUT:?}");
            }
            let output = Command::new("curl")
                .args([
                    "-s",
                    "-o",
                    "/dev/null",
                    "-w",
                    "%{http_code}",
                    "--max-time",
                    "1",
                    &format!("http://127.0.0.1:{port}/health"),
                ])
                .output();
            if output.is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "200") {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn captured_log(&self) -> String {
        let mut contents = String::new();
        if let Ok(mut file) = std::fs::File::open(&self.log_path) {
            let _ = file.read_to_string(&mut contents);
        }
        strip_ansi(&contents)
    }
}

/// Strip ANSI/CSI escape sequences (`\x1b[...<letter>`) from `soma serve`'s
/// captured subprocess log before substring-matching it.
///
/// `tracing_subscriber::fmt()` defaults to ANSI-colorized output unless
/// `.with_ansi(false)` is explicitly set (see
/// `apps/soma/src/bootstrap.rs::init_logging`) — confirmed empirically that
/// this holds even when stdout/stderr are redirected to a plain file, not
/// just a real terminal. The in-process tracing captures in
/// `apps/soma/tests/mcp_trace_headers.rs` sidestep this by building their own
/// subscriber with `.with_ansi(false)`, but this smoke spawns a real `soma`
/// subprocess and can only observe its already-formatted stdout/stderr text,
/// so color codes land *inside* field name/value boundaries (e.g.
/// `trace_id_prefix` `<ESC>[0m<ESC>[2m` `=` `<ESC>[0m` `Some(...)`) and silently
/// break a naive `contains(...)` check — every "logs contain X" assertion
/// failed until this was added, even though the underlying trace data was
/// correct (verified by hand against the raw captured log).
fn strip_ansi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        output.push(ch);
    }
    output
}

#[cfg(test)]
mod strip_ansi_tests {
    use super::strip_ansi;

    #[test]
    fn removes_sgr_escape_sequences_between_field_name_and_value() {
        let colored = "\u{1b}[3mtrace_id_prefix\u{1b}[0m\u{1b}[2m=\u{1b}[0mSome(\"0af76519\")";
        assert_eq!(strip_ansi(colored), "trace_id_prefix=Some(\"0af76519\")");
    }

    #[test]
    fn leaves_plain_text_untouched() {
        assert_eq!(
            strip_ansi("http_trace_headers_present=false"),
            "http_trace_headers_present=false"
        );
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_curl_uses_the_request_timeout() {
        let command = curl_preflight_command(40060, "TraceParent");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect();
        let timeout_index = args
            .iter()
            .position(|arg| arg == "--max-time")
            .expect("preflight curl should have --max-time");
        assert_eq!(
            args.get(timeout_index + 1).map(|arg| arg.as_ref()),
            Some("5")
        );
    }

    #[cfg(unix)]
    #[test]
    fn preflight_rejects_a_nonzero_curl_exit_status() {
        use std::os::unix::process::ExitStatusExt;

        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(7 << 8),
            stdout: Vec::new(),
            stderr: b"timed out".to_vec(),
        };

        let error = checked_preflight_output(output).expect_err("nonzero curl must fail smoke");
        assert!(error.to_string().contains("curl preflight failed"));
        assert!(error.to_string().contains("timed out"));
    }
}
