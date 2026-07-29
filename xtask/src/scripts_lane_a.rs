//! Lane A script migrations.
//!
//! These functions are intentionally not wired into `main.rs` in this lane.
//! They preserve the behavior of the compatibility shell scripts and are ready
//! for the parent integration pass to expose as xtask commands.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn build_web() -> Result<()> {
    let web_dir = Path::new("apps/web");
    if !web_dir.is_dir() {
        println!("No apps/web directory found - skipping web build");
        return Ok(());
    }

    if !web_dir.join("node_modules").is_dir() {
        println!("Installing web dependencies...");
        run_cmd_in("pnpm", ["install", "--frozen-lockfile"], web_dir)?;
    }

    run_cmd_in("pnpm", ["build"], web_dir)?;
    println!("Web assets built -> apps/web/out/");
    Ok(())
}

pub fn web_watch() -> Result<()> {
    web_watch_with_build_command("bash scripts/build-web.sh")
}

pub fn web_watch_with_build_command(build_command: &str) -> Result<()> {
    if !command_on_path("watchexec") {
        eprintln!("error: watchexec is required for web-watch");
        eprintln!("install: cargo install watchexec-cli");
        bail!("watchexec is required for web-watch");
    }

    println!("Building apps/web once...");
    run_cmd("bash", ["scripts/build-web.sh"])?;

    println!("Watching apps/web for changes...");
    let args = web_watch_args(build_command);
    run_command(Command::new("watchexec").args(args))
}

pub fn generate_cli() -> Result<()> {
    if !command_on_path("mcporter") {
        eprintln!("error: mcporter not found. Install it first.");
        bail!("mcporter not found");
    }

    println!("Server must be running on port 40060 (run 'just dev' first)");
    println!("Generated CLI embeds your token - do not commit or share");

    fs::create_dir_all("dist/.cache").context("failed to create dist/.cache")?;

    let schema_json = temp_schema_path();
    let _guard = RemoveOnDrop(schema_json.clone());
    let token = std::env::var("SOMA_MCP_TOKEN")
        .ok()
        .filter(|v| !v.is_empty());
    let request = GenerateCliRequest::new(token.as_deref());

    let mut curl_args: Vec<OsString> = vec!["10".into(), "curl".into(), "-sf".into()];
    curl_args.extend(request.curl_headers.iter().cloned());
    curl_args.extend([
        "http://localhost:40060/mcp/tools/list".into(),
        "-o".into(),
        schema_json.as_os_str().to_owned(),
    ]);
    run_cmd_os("timeout", curl_args).with_context(
        || "error: failed to fetch tool schema from http://localhost:40060/mcp/tools/list",
    )?;

    let current_hash = sha256sum(&schema_json)?;
    let cache_file = Path::new("dist/.cache/soma-cli.schema_hash");
    if cached_cli_is_current(cache_file, Path::new("dist/soma-cli"), &current_hash)? {
        println!("SKIP: tool schema unchanged - use existing dist/soma-cli");
        return Ok(());
    }

    let mut mcporter_args: Vec<OsString> = vec!["30".into(), "mcporter".into()];
    mcporter_args.extend(request.mcporter_args);
    run_cmd_os("timeout", mcporter_args)?;

    set_private_executable(Path::new("dist/soma-cli"))?;
    if !git_check_ignore(Path::new("dist/soma-cli")) {
        eprintln!(
            "warning: dist/soma-cli is not ignored; generated CLI embeds secrets and must not be committed"
        );
    }

    fs::write(cache_file, current_hash).context("failed to write CLI schema hash cache")?;
    println!("Generated dist/soma-cli (requires bun at runtime)");
    Ok(())
}

pub fn repair() -> Result<()> {
    println!("==> Stopping soma-mcp...");
    if systemd_user_unit_active("soma-mcp.service") {
        run_cmd("systemctl", ["--user", "stop", "soma-mcp.service"])?;
        println!("    stopped systemd unit");
    } else if docker_container_running("soma-mcp") {
        let _ = run_cmd("docker", ["stop", "soma-mcp"]);
        println!("    stopped docker container");
    } else {
        println!("    no running instance found");
    }

    println!("==> Rebuilding release binary...");
    run_cmd(
        "cargo",
        ["build", "--release", "--bin", "soma", "--features", "full"],
    )?;

    println!("==> Restarting...");
    if systemd_user_unit_file_exists("soma-mcp.service") {
        let home = std::env::var("HOME").context("HOME is not set")?;
        let bin_dir = Path::new(&home).join(".local/bin");
        fs::create_dir_all(&bin_dir)
            .with_context(|| format!("failed to create {}", bin_dir.display()))?;
        run_cmd(
            "install",
            [
                "-m",
                "755",
                "target/release/soma",
                bin_dir
                    .join("soma")
                    .to_str()
                    .context("non-UTF-8 install path")?,
            ],
        )?;
        run_cmd("systemctl", ["--user", "start", "soma-mcp.service"])?;
        println!("    started systemd unit");
    } else if Path::new("docker-compose.yml").is_file() {
        run_cmd("docker", ["compose", "build"])?;
        run_cmd("docker", ["compose", "up", "-d", "--force-recreate"])?;
        println!("    started docker compose service");
    } else {
        println!("    no service manager detected; binary at target/release/soma");
    }

    println!("==> Done");
    Ok(())
}

pub fn test_mcp_auth(args: &[String]) -> Result<()> {
    let options = AuthSmokeOptions::parse(args)?;
    if options.help {
        print_auth_usage();
        return Ok(());
    }
    let token = options
        .token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            eprintln!("ERROR: set SOMA_MCP_TOKEN or pass --token");
            anyhow::anyhow!("missing SOMA_MCP_TOKEN")
        })?;

    let body_path = Path::new("/tmp/soma-auth-body.txt");
    let _guard = RemoveOnDrop(body_path.to_path_buf());
    let request_body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
    let base_url = base_url_from_mcp_url(&options.mcp_url);
    let mut results = AuthSmokeResults::default();

    expect_code(
        &mut results,
        "health is public",
        "200",
        body_path,
        &options.timeout,
        [format!("{base_url}/health")],
    );

    expect_code(
        &mut results,
        "missing bearer token is rejected",
        "401",
        body_path,
        &options.timeout,
        post_jsonrpc_args(&options.mcp_url, None, request_body),
    );

    expect_code(
        &mut results,
        "bad bearer token is rejected",
        "401",
        body_path,
        &options.timeout,
        post_jsonrpc_args(
            &options.mcp_url,
            Some(("Authorization", "Bearer intentionally-bad-token".to_owned())),
            request_body,
        ),
    );

    expect_success_jsonrpc(
        &mut results,
        "valid bearer token is accepted",
        body_path,
        &options.timeout,
        post_jsonrpc_args(
            &options.mcp_url,
            Some(("Authorization", format!("Bearer {token}"))),
            request_body,
        ),
    );

    if options.check_x_api_key {
        expect_success_jsonrpc(
            &mut results,
            "x-api-key token is accepted",
            body_path,
            &options.timeout,
            post_jsonrpc_args(
                &options.mcp_url,
                Some(("x-api-key", token.to_owned())),
                request_body,
            ),
        );
    } else {
        println!(
            "SKIP  x-api-key acceptance (pass --check-x-api-key only for services that implement it)"
        );
    }

    println!("\n{} passed, {} failed", results.pass, results.fail);
    if results.fail == 0 {
        Ok(())
    } else {
        bail!("{} auth smoke check(s) failed", results.fail)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GenerateCliRequest {
    curl_headers: Vec<OsString>,
    mcporter_args: Vec<OsString>,
}

impl GenerateCliRequest {
    fn new(token: Option<&str>) -> Self {
        let mut curl_headers = vec![
            "-H".into(),
            "Accept: application/json, text/event-stream".into(),
        ];
        let mut mcporter_args: Vec<OsString> = [
            "generate-cli",
            "--command",
            "http://localhost:40060/mcp",
            "--name",
            "soma-cli",
            "--output",
            "dist/soma-cli",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();

        if let Some(token) = token {
            let auth = format!("Authorization: Bearer {token}");
            curl_headers.extend(["-H".into(), auth.clone().into()]);
            mcporter_args.extend(["--header".into(), auth.into()]);
        }

        Self {
            curl_headers,
            mcporter_args,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AuthSmokeOptions {
    mcp_url: String,
    token: Option<String>,
    timeout: String,
    check_x_api_key: bool,
    help: bool,
}

impl AuthSmokeOptions {
    fn parse(args: &[String]) -> Result<Self> {
        let mut options = Self {
            mcp_url: std::env::var("SOMA_MCP_URL")
                .unwrap_or_else(|_| "http://localhost:40060/mcp".to_owned()),
            token: std::env::var("SOMA_MCP_TOKEN").ok(),
            timeout: std::env::var("MCP_AUTH_TIMEOUT").unwrap_or_else(|_| "10".to_owned()),
            check_x_api_key: false,
            help: false,
        };

        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--url" => {
                    index += 1;
                    options.mcp_url = args
                        .get(index)
                        .context("--url requires a value")?
                        .to_owned();
                }
                "--token" => {
                    index += 1;
                    options.token = Some(
                        args.get(index)
                            .context("--token requires a value")?
                            .to_owned(),
                    );
                }
                "--check-x-api-key" => options.check_x_api_key = true,
                "-h" | "--help" => options.help = true,
                unknown => {
                    eprintln!("unknown argument: {unknown}");
                    print_auth_usage();
                    bail!("unknown argument: {unknown}");
                }
            }
            index += 1;
        }

        Ok(options)
    }
}

#[derive(Default)]
pub(crate) struct AuthSmokeResults {
    pub(crate) pass: usize,
    pub(crate) fail: usize,
}

impl AuthSmokeResults {
    pub(crate) fn pass(&mut self, label: &str) {
        println!("PASS  {label}");
        self.pass += 1;
    }

    pub(crate) fn fail(&mut self, label: &str) {
        eprintln!("FAIL  {label}");
        self.fail += 1;
    }
}

fn print_auth_usage() {
    println!(
        "Usage: scripts/test-mcp-auth.sh [OPTIONS]

Options:
  --url URL              MCP URL. Default: SOMA_MCP_URL or http://localhost:40060/mcp.
  --token TOKEN          Expected static bearer token. Default: SOMA_MCP_TOKEN.
  --check-x-api-key      Also require x-api-key auth to succeed. Off by default because
                         Soma's pinned lab-auth layer only supports Bearer.
  -h, --help             Show this help.

Checks:
  - /health is reachable without auth
  - /mcp rejects missing bearer token with 401
  - /mcp rejects a bad bearer token with 401
  - /mcp accepts Authorization: Bearer <token>
  - x-api-key is skipped unless --check-x-api-key is set"
    );
}

fn expect_code<I, S>(
    results: &mut AuthSmokeResults,
    label: &str,
    expected: &str,
    body_path: &Path,
    timeout: &str,
    args: I,
) where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match http_code(body_path, timeout, args) {
        Ok(code) if code == expected => results.pass(label),
        Ok(code) => results.fail(&format!(
            "{label} (expected HTTP {expected}, got {code}; body: {})",
            body_preview(body_path)
        )),
        Err(_) => results.fail(&format!("{label} (curl failed)")),
    }
}

fn expect_success_jsonrpc<I, S>(
    results: &mut AuthSmokeResults,
    label: &str,
    body_path: &Path,
    timeout: &str,
    args: I,
) where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match http_code(body_path, timeout, args) {
        Ok(code) if code == "200" => match response_has_tools(body_path) {
            Ok(true) => results.pass(label),
            Ok(false) => results.fail(&format!("{label} (missing tools)")),
            Err(error) => results.fail(&format!("{label} (parse error: {error})")),
        },
        Ok(code) => results.fail(&format!(
            "{label} (expected HTTP 200, got {code}; body: {})",
            body_preview(body_path)
        )),
        Err(_) => results.fail(&format!("{label} (curl failed)")),
    }
}

fn http_code<I, S>(body_path: &Path, timeout: &str, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let output = Command::new("curl")
        .arg("-sS")
        .arg("--max-time")
        .arg(timeout)
        .arg("-o")
        .arg(body_path)
        .arg("-w")
        .arg("%{http_code}")
        .args(args.into_iter().map(Into::into))
        .stdin(Stdio::null())
        .output()
        .context("failed to spawn curl")?;
    if !output.status.success() {
        bail!("curl exited with {}", output.status);
    }
    String::from_utf8(output.stdout).context("curl emitted non-UTF-8 status")
}

fn response_has_tools(body_path: &Path) -> Result<bool> {
    let body = fs::read_to_string(body_path)
        .with_context(|| format!("failed to read {}", body_path.display()))?;
    let value: Value = serde_json::from_str(&body)?;
    Ok(value
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty()))
}

fn post_jsonrpc_args(
    mcp_url: &str,
    auth_header: Option<(&str, String)>,
    request_body: &str,
) -> Vec<OsString> {
    let mut args = vec![
        "-X".into(),
        "POST".into(),
        mcp_url.into(),
        "-H".into(),
        "Content-Type: application/json".into(),
        "-H".into(),
        "Accept: application/json, text/event-stream".into(),
        "-d".into(),
        request_body.into(),
    ];
    if let Some((name, value)) = auth_header {
        args.splice(3..3, ["-H".into(), format!("{name}: {value}").into()]);
    }
    args
}

fn base_url_from_mcp_url(mcp_url: &str) -> String {
    mcp_url.strip_suffix("/mcp").unwrap_or(mcp_url).to_owned()
}

fn body_preview(body_path: &Path) -> String {
    fs::read_to_string(body_path)
        .unwrap_or_default()
        .chars()
        .filter(|ch| *ch != '\n')
        .take(200)
        .collect()
}

fn web_watch_args(build_command: &str) -> Vec<OsString> {
    [
        "--project-origin",
        ".",
        "--watch",
        "apps/web",
        "--ignore",
        "apps/web/.next",
        "--ignore",
        "apps/web/.next/**",
        "--ignore",
        "apps/web/out",
        "--ignore",
        "apps/web/out/**",
        "--ignore",
        "apps/web/node_modules",
        "--ignore",
        "apps/web/node_modules/**",
        "--debounce",
        "1000ms",
        "--on-busy-update",
        "queue",
        "--wrap-process=none",
        build_command,
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

fn temp_schema_path() -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    path.push(format!("soma-schema-{}-{nanos}.json", std::process::id()));
    path
}

fn cached_cli_is_current(cache_file: &Path, cli_path: &Path, current_hash: &str) -> Result<bool> {
    if !cache_file.is_file() || !cli_path.is_file() {
        return Ok(false);
    }
    let cached_hash = fs::read_to_string(cache_file)
        .with_context(|| format!("failed to read {}", cache_file.display()))?;
    Ok(cached_hash == current_hash)
}

fn sha256sum(path: &Path) -> Result<String> {
    let output = Command::new("sha256sum")
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .context("failed to spawn sha256sum")?;
    if !output.status.success() {
        bail!("sha256sum exited with {}", output.status);
    }
    let stdout = String::from_utf8(output.stdout).context("sha256sum emitted non-UTF-8 stdout")?;
    stdout
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .context("sha256sum did not print a hash")
}

fn git_check_ignore(path: &Path) -> bool {
    Command::new("git")
        .arg("check-ignore")
        .arg("-q")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn systemd_user_unit_active(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", unit])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn systemd_user_unit_file_exists(unit: &str) -> bool {
    let output = Command::new("systemctl")
        .args(["--user", "list-unit-files", unit])
        .stdin(Stdio::null())
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    String::from_utf8(output.stdout)
        .map(|stdout| stdout.contains(unit))
        .unwrap_or(false)
}

fn docker_container_running(name: &str) -> bool {
    let filter = format!("name=^/{name}$");
    let output = Command::new("docker")
        .args(["ps", "--filter", &filter, "--quiet"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else {
        return false;
    };
    output.status.success() && !output.stdout.iter().all(u8::is_ascii_whitespace)
}

fn command_on_path(name: &str) -> bool {
    if name.contains('/') {
        return Path::new(name).is_file();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
}

fn run_cmd<I, S>(program: &str, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_command(Command::new(program).args(args))
}

fn run_cmd_os<I>(program: &str, args: I) -> Result<()>
where
    I: IntoIterator<Item = OsString>,
{
    run_command(Command::new(program).args(args))
}

fn run_cmd_in<I, S>(program: &str, args: I, cwd: &Path) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_command(Command::new(program).args(args).current_dir(cwd))
}

fn run_command(command: &mut Command) -> Result<()> {
    let program = command.get_program().to_string_lossy().into_owned();
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    let status = command
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("Failed to spawn `{program}`"))?;
    if !status.success() {
        bail!("`{program} {args}` exited with status {status}");
    }
    Ok(())
}

#[cfg(unix)]
fn set_private_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to chmod 700 {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_executable(path: &Path) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        bail!("generated CLI missing at {}", path.display())
    }
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuthSmokeOptions, GenerateCliRequest, base_url_from_mcp_url, cached_cli_is_current,
        post_jsonrpc_args, response_has_tools, web_watch_args,
    };
    use std::ffi::OsString;
    use std::fs;

    fn os_strings(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn web_watch_args_preserve_script_ignores_and_build_command() {
        let args = web_watch_args("bash scripts/build-web.sh");
        assert_eq!(args[0], "--project-origin");
        assert!(args.contains(&OsString::from("apps/web/.next/**")));
        assert!(args.contains(&OsString::from("apps/web/out/**")));
        assert!(args.contains(&OsString::from("apps/web/node_modules/**")));
        assert_eq!(
            args.last(),
            Some(&OsString::from("bash scripts/build-web.sh"))
        );
    }

    #[test]
    fn generate_cli_request_adds_auth_headers_only_when_token_is_present() {
        let without_token = GenerateCliRequest::new(None);
        assert_eq!(
            without_token.curl_headers,
            os_strings(&["-H", "Accept: application/json, text/event-stream"])
        );
        assert!(
            !without_token
                .mcporter_args
                .contains(&OsString::from("--header"))
        );

        let with_token = GenerateCliRequest::new(Some("secret"));
        assert!(
            with_token
                .curl_headers
                .contains(&OsString::from("Authorization: Bearer secret"))
        );
        assert!(with_token.mcporter_args.windows(2).any(|pair| {
            pair == [
                OsString::from("--header"),
                OsString::from("Authorization: Bearer secret"),
            ]
        }));
    }

    #[test]
    fn auth_options_parse_flags_over_defaults() {
        let args = vec![
            "--url".to_owned(),
            "http://example.test/mcp".to_owned(),
            "--token".to_owned(),
            "expected".to_owned(),
            "--check-x-api-key".to_owned(),
        ];
        let options = AuthSmokeOptions::parse(&args).unwrap();
        assert_eq!(options.mcp_url, "http://example.test/mcp");
        assert_eq!(options.token.as_deref(), Some("expected"));
        assert_eq!(options.timeout, "10");
        assert!(options.check_x_api_key);
        assert!(!options.help);
    }

    #[test]
    fn auth_options_reject_unknown_args() {
        let error = AuthSmokeOptions::parse(&["--wat".to_owned()]).unwrap_err();
        assert!(error.to_string().contains("unknown argument"));
    }

    #[test]
    fn base_url_strips_only_trailing_mcp_segment() {
        assert_eq!(
            base_url_from_mcp_url("http://localhost:40060/mcp"),
            "http://localhost:40060"
        );
        assert_eq!(
            base_url_from_mcp_url("http://localhost:40060/custom"),
            "http://localhost:40060/custom"
        );
    }

    #[test]
    fn post_jsonrpc_args_insert_auth_header_after_url() {
        let args = post_jsonrpc_args(
            "http://localhost:40060/mcp",
            Some(("Authorization", "Bearer token".to_owned())),
            "{}",
        );
        assert_eq!(args[0], "-X");
        assert_eq!(args[2], "http://localhost:40060/mcp");
        assert_eq!(args[3], "-H");
        assert_eq!(args[4], "Authorization: Bearer token");
        assert!(args.contains(&OsString::from("Content-Type: application/json")));
        assert!(args.contains(&OsString::from(
            "Accept: application/json, text/event-stream"
        )));
    }

    #[test]
    fn response_has_tools_requires_non_empty_tools_array() {
        let dir = tempfile::tempdir().unwrap();
        let body = dir.path().join("body.json");

        fs::write(&body, r#"{"result":{"tools":[{"name":"soma"}]}}"#).unwrap();
        assert!(response_has_tools(&body).unwrap());

        fs::write(&body, r#"{"result":{"tools":[]}}"#).unwrap();
        assert!(!response_has_tools(&body).unwrap());
    }

    #[test]
    fn cached_cli_requires_matching_hash_and_cli_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("hash");
        let cli = dir.path().join("soma-cli");

        assert!(!cached_cli_is_current(&cache, &cli, "abc").unwrap());

        fs::write(&cache, "abc").unwrap();
        assert!(!cached_cli_is_current(&cache, &cli, "abc").unwrap());

        fs::write(&cli, "").unwrap();
        assert!(cached_cli_is_current(&cache, &cli, "abc").unwrap());
        assert!(!cached_cli_is_current(&cache, &cli, "def").unwrap());
    }
}
