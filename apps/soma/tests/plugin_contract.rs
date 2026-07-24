use serde_json::Value;
use std::fs;
use std::process::Command;

use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn read(path: &str) -> String {
    fs::read_to_string(repo_path(path)).unwrap_or_else(|err| panic!("failed to read {path}: {err}"))
}

fn json(path: &str) -> Value {
    serde_json::from_str(&read(path)).unwrap_or_else(|err| panic!("failed to parse {path}: {err}"))
}

fn repo_path(path: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn plugin_manifests_exist_for_all_supported_hosts() {
    for path in [
        "plugins/soma/.claude-plugin/plugin.json",
        "plugins/soma/.codex-plugin/plugin.json",
        "plugins/soma/gemini-extension.json",
        "plugins/soma/hooks/hooks.json",
        "plugins/soma/skills/soma/SKILL.md",
    ] {
        assert!(repo_path(path).exists(), "{path} should exist");
    }
}

#[test]
fn plugin_manifests_share_identity_and_connection_settings() {
    let claude = json("plugins/soma/.claude-plugin/plugin.json");
    let codex = json("plugins/soma/.codex-plugin/plugin.json");
    let gemini = json("plugins/soma/gemini-extension.json");

    assert_eq!(claude["name"], "soma");
    assert_eq!(codex["name"], "soma");
    assert_eq!(gemini["name"], "soma");
    assert!(
        claude.get("experimental").is_none(),
        "stdio-first plugin should not auto-register HTTP health monitors"
    );

    assert!(claude["repository"].as_str().unwrap().ends_with("soma"));
    assert!(codex["repository"].as_str().unwrap().ends_with("soma"));
    assert!(gemini["repository"].as_str().unwrap().ends_with("soma"));
    assert_eq!(claude["homepage"], "https://soma.dinglebear.ai");
    assert_eq!(codex["homepage"], "https://soma.dinglebear.ai");
    assert_eq!(gemini["homepage"], "https://soma.dinglebear.ai");
    for manifest in [&claude, &codex, &gemini] {
        assert!(
            manifest["keywords"]
                .as_array()
                .unwrap()
                .iter()
                .any(|keyword| keyword == "provider-runtime"),
            "plugin metadata should advertise provider-runtime discoverability"
        );
    }

    let user_config = claude["userConfig"].as_object().unwrap();
    for key in [
        "server_url",
        "api_token",
        "soma_api_url",
        "soma_api_key",
        "trace_headers",
    ] {
        assert!(
            user_config.contains_key(key),
            "Claude userConfig missing {key}"
        );
    }

    let gemini_settings: Vec<&str> = gemini["settings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|setting| setting["name"].as_str().unwrap())
        .collect();
    for key in [
        "server_url",
        "api_token",
        "soma_api_url",
        "soma_api_key",
        "trace_headers",
    ] {
        assert!(
            gemini_settings.contains(&key),
            "Gemini settings missing {key}"
        );
    }

    // Marketplace manifests intentionally omit MCP server registration
    // (see plugins/README.md); there is no .mcp.json or mcpServers block to assert.
}

#[test]
fn codex_plugin_icon_assets_exist() {
    let codex = json("plugins/soma/.codex-plugin/plugin.json");
    for pointer in ["/interface/composerIcon", "/interface/logo"] {
        let asset = codex.pointer(pointer).and_then(Value::as_str).unwrap();
        let relative = asset.strip_prefix("./").unwrap_or(asset);
        assert!(
            repo_path("plugins/soma").join(relative).is_file(),
            "{pointer} should point at an existing plugin asset"
        );
    }
}

#[test]
fn mcp_registry_manifest_advertises_rich_product_metadata() {
    let manifest = json("server.json");
    assert_eq!(manifest["name"], "ai.dinglebear/soma");
    assert_eq!(manifest["title"], "Soma");
    assert_eq!(
        manifest["repository"]["url"],
        "https://github.com/dinglebear-ai/soma"
    );
    assert_eq!(manifest["repository"]["id"], "1238227299");
    assert_eq!(manifest["websiteUrl"], "https://soma.dinglebear.ai");
    assert_eq!(manifest["_meta"]["ai.dinglebear.soma"]["binary"], "soma");
    assert_eq!(
        manifest["_meta"]["ai.dinglebear.soma"]["homepage"],
        "https://soma.dinglebear.ai"
    );
    assert_eq!(
        manifest["_meta"]["ai.dinglebear.soma"]["support_url"],
        "https://github.com/dinglebear-ai/soma/issues"
    );
    assert!(
        manifest["_meta"]["ai.dinglebear.soma"]["keywords"]
            .as_array()
            .unwrap()
            .iter()
            .any(|keyword| keyword == "provider-runtime"),
        "server metadata should advertise provider-runtime discoverability"
    );
    assert_eq!(
        manifest["_meta"]["ai.dinglebear.soma"]["server_binary"],
        "soma"
    );
    assert_eq!(
        manifest["_meta"]["io.modelcontextprotocol.registry/publisher-provided"]["publisher"]
            ["name"],
        "dinglebear.ai"
    );
    assert!(
        manifest["icons"].as_array().unwrap().len() >= 2,
        "server.json should advertise PNG and SVG icons"
    );

    let packages = manifest["packages"].as_array().unwrap();
    let oci = packages
        .iter()
        .find(|package| package["registryType"] == "oci")
        .expect("missing OCI package metadata");
    assert_eq!(oci["identifier"], "ghcr.io/dinglebear-ai/soma:0.5.0");
    assert_eq!(oci["runtimeHint"], "docker");
    assert_eq!(oci["transport"]["type"], "stdio");
    assert!(
        oci.get("version").is_none(),
        "OCI packages encode the version in the image tag"
    );
    assert!(
        oci.get("registryBaseUrl").is_none(),
        "OCI packages encode the registry in the canonical image reference"
    );
    assert!(oci["packageArguments"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg["value"] == "mcp"));

    let oci_envs: Vec<&str> = oci["environmentVariables"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|env| env["name"].as_str())
        .collect();
    for name in [
        "SOMA_HOME",
        "SOMA_PROVIDER_DIR",
        "SOMA_API_URL",
        "SOMA_API_KEY",
        "RUST_LOG",
    ] {
        assert!(oci_envs.contains(&name), "OCI metadata missing {name}");
    }

    assert_eq!(
        manifest["_meta"]["io.modelcontextprotocol.registry/publisher-provided"]["distribution"]
            ["ociImage"],
        oci["identifier"]
    );
}

#[test]
fn npm_launcher_package_has_distribution_metadata() {
    let package = json("packages/soma-rmcp/package.json");
    assert_eq!(package["name"], "soma-rmcp");
    assert_eq!(package["mcpName"], "ai.dinglebear/soma");
    assert_eq!(package["homepage"], "https://soma.dinglebear.ai");
    assert_eq!(package["author"]["name"], "dinglebear.ai");
    assert_eq!(package["repository"]["directory"], "packages/soma-rmcp");
    assert_eq!(
        package["bugs"]["url"],
        "https://github.com/dinglebear-ai/soma/issues"
    );
    assert_eq!(package["bin"]["soma"], "bin/soma-rmcp.js");
    assert_eq!(package["bin"]["soma-rmcp"], "bin/soma-rmcp.js");
}

#[test]
fn generated_openapi_carries_product_metadata() {
    let openapi = json("docs/generated/openapi.json");
    assert_eq!(openapi["info"]["contact"]["name"], "dinglebear.ai");
    assert_eq!(openapi["info"]["license"]["name"], "MIT");
    assert_eq!(
        openapi["externalDocs"]["url"],
        "https://github.com/dinglebear-ai/soma/tree/main/docs"
    );
    assert_eq!(openapi["x-soma"]["binary"], "soma");
    assert_eq!(openapi["x-soma"]["node_package"], "soma-rmcp");
    assert_eq!(openapi["x-soma"]["mcp_registry"], "server.json");
    assert_eq!(openapi["x-soma"]["publisher"]["name"], "dinglebear.ai");
    assert!(openapi["x-soma"]["auth_modes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|mode| mode == "oauth"));
}

#[test]
fn claude_hooks_call_binary_setup_plugin_hook_directly() {
    let hooks = json("plugins/soma/hooks/hooks.json");
    for hook_name in ["SessionStart", "ConfigChange"] {
        let command = hooks["hooks"][hook_name][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert_eq!(command, "soma setup plugin-hook");
    }
}

#[test]
fn plugin_hook_standard_is_documented() {
    let plugins = read("docs/PLUGINS.md");
    let patterns = read("docs/PATTERNS.md");
    for doc in [plugins, patterns] {
        assert!(doc.contains("<binary> setup plugin-hook"));
        assert!(doc.contains("<binary> setup plugin-hook --no-repair"));
        assert!(doc.contains("exit_policy"));
        assert!(doc.contains("blocking_failures"));
        assert!(doc.contains("advisory_failures"));
        assert!(doc.contains("ran_repair"));
    }
}

fn example_bin() -> std::path::PathBuf {
    const BIN_NAME: &str = "soma";
    let key = format!("CARGO_BIN_EXE_{BIN_NAME}");
    let alt_key = format!("CARGO_BIN_EXE_{}", BIN_NAME.replace('-', "_"));
    std::env::var_os(&key)
        .or_else(|| std::env::var_os(&alt_key))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(BIN_NAME))
}

fn free_loopback_port() -> u16 {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("should bind to an ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn setup_command(data_dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(example_bin());
    let port = free_loopback_port().to_string();
    cmd.env_clear()
        .env("HOME", data_dir)
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("SOMA_HOME", data_dir)
        .env("SOMA_API_URL", "https://api.example.test")
        .env("SOMA_API_KEY", "example-secret")
        .env("SOMA_MCP_HOST", "127.0.0.1")
        .env("SOMA_MCP_PORT", port)
        .env("SOMA_MCP_TOKEN", "mcp-secret");
    cmd
}

fn assert_repair_success_or_windows_port_advisory(json: &Value) {
    if cfg!(windows) && json["exit_policy"] == "advisory_failure" {
        let codes: Vec<&str> = json["advisory_failures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|failure| failure["code"].as_str().unwrap())
            .collect();
        assert_eq!(
            codes,
            ["mcp_port_in_use"],
            "unexpected advisory failures after setup repair: {json:#}"
        );
    } else {
        assert_eq!(
            json["exit_policy"], "success",
            "setup repair JSON: {json:#}"
        );
    }
}

#[test]
fn setup_plugin_hook_no_repair_emits_json_contract() {
    let dir = tempdir().unwrap();
    let mut cmd = setup_command(dir.path());
    let output = cmd
        .args(["setup", "plugin-hook", "--no-repair"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["exit_policy"], "advisory_failure");
    assert_eq!(json["ran_repair"], false);
    assert_eq!(json["no_repair"], true);
    assert!(json["blocking_failures"].as_array().unwrap().is_empty());
    assert!(json["advisory_failures"]
        .as_array()
        .unwrap()
        .iter()
        .any(|failure| failure["code"] == "env_file_missing"));
    assert!(!dir.path().join(".env").exists());
}

#[test]
fn setup_repair_creates_env_file_without_upstream_contact() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("appdata");
    let mut cmd = setup_command(&missing);
    let output = cmd.args(["setup", "repair"]).output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_repair_success_or_windows_port_advisory(&json);
    assert_eq!(json["ran_repair"], true);
    assert_eq!(json["no_repair"], false);

    let env_file = std::fs::read_to_string(missing.join(".env")).unwrap();
    assert!(env_file.contains("SOMA_API_URL=https://api.example.test"));
    assert!(env_file.contains("SOMA_API_KEY=example-secret"));
    assert!(env_file.contains("SOMA_MCP_TOKEN=mcp-secret"));
    assert_env_file_mode(missing.join(".env").as_path());
}

#[test]
fn setup_repair_replaces_existing_env_file_with_private_mode() {
    let dir = tempdir().unwrap();
    let env_path = dir.path().join(".env");
    fs::write(&env_path, "OLD_VALUE=1\n").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&env_path, fs::Permissions::from_mode(0o644)).unwrap();

    let mut cmd = setup_command(dir.path());
    let output = cmd.args(["setup", "repair"]).output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let env_file = fs::read_to_string(&env_path).unwrap();
    assert!(!env_file.contains("OLD_VALUE"));
    assert!(env_file.contains("SOMA_API_URL=https://api.example.test"));
    assert_env_file_mode(&env_path);
}

fn assert_env_file_mode(path: &std::path::Path) {
    #[cfg(unix)]
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

// ── OAuth setup validation (H12) ─────────────────────────────────────────────
//
// These helpers build a Command with OAuth mode enabled and all four OAuth
// credentials present, then selectively omit one field per test to confirm
// the expected blocking-failure code is reported by `setup plugin-hook
// --no-repair`.
//
// Notes:
//   - `setup_command` sets SOMA_MCP_TOKEN, which normally selects bearer
//     mode.  We override that by adding SOMA_MCP_AUTH_MODE=oauth.
//   - We omit SOMA_MCP_TOKEN here so the setup logic enters the OAuth
//     credential-check branch (token takes precedence in bearer mode).
//   - The port is assigned from an ephemeral loopback bind to avoid
//     mcp_port_in_use noise from fixed test ports.

fn oauth_setup_command(data_dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(example_bin());
    let port = free_loopback_port().to_string();
    cmd.env_clear()
        .env("HOME", data_dir)
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("SOMA_HOME", data_dir)
        .env("SOMA_API_URL", "https://api.example.test")
        .env("SOMA_API_KEY", "example-secret")
        .env("SOMA_MCP_HOST", "127.0.0.1")
        .env("SOMA_MCP_PORT", port)
        .env("SOMA_MCP_AUTH_MODE", "oauth")
        .env("SOMA_MCP_PUBLIC_URL", "https://mcp.example.test")
        .env("SOMA_MCP_GOOGLE_CLIENT_ID", "test-client-id")
        .env("SOMA_MCP_GOOGLE_CLIENT_SECRET", "test-client-secret")
        .env("SOMA_MCP_AUTH_ADMIN_EMAIL", "admin@example.test");
    cmd
}

fn blocking_failure_codes(output: &std::process::Output) -> Vec<String> {
    let json: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout not JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    json["blocking_failures"]
        .as_array()
        .expect("blocking_failures should be an array")
        .iter()
        .map(|f| f["code"].as_str().unwrap_or("").to_string())
        .collect()
}

#[test]
fn oauth_missing_public_url_produces_blocking_failure() {
    let dir = tempdir().unwrap();
    let mut cmd = oauth_setup_command(dir.path());
    // Remove the public URL so the check fires.
    cmd.env_remove("SOMA_MCP_PUBLIC_URL");
    let output = cmd
        .args(["setup", "plugin-hook", "--no-repair"])
        .output()
        .unwrap();

    // setup exits non-zero when there are blocking failures.
    assert!(
        !output.status.success(),
        "expected non-zero exit for blocking failure; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let codes = blocking_failure_codes(&output);
    assert!(
        codes.contains(&"missing_oauth_public_url".to_string()),
        "expected missing_oauth_public_url in blocking_failures, got: {codes:?}"
    );
}

#[test]
fn oauth_missing_client_id_produces_blocking_failure() {
    let dir = tempdir().unwrap();
    let mut cmd = oauth_setup_command(dir.path());
    cmd.env_remove("SOMA_MCP_GOOGLE_CLIENT_ID");
    let output = cmd
        .args(["setup", "plugin-hook", "--no-repair"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit for blocking failure; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let codes = blocking_failure_codes(&output);
    assert!(
        codes.contains(&"missing_oauth_client_id".to_string()),
        "expected missing_oauth_client_id in blocking_failures, got: {codes:?}"
    );
}

#[test]
fn oauth_missing_client_secret_produces_blocking_failure() {
    let dir = tempdir().unwrap();
    let mut cmd = oauth_setup_command(dir.path());
    cmd.env_remove("SOMA_MCP_GOOGLE_CLIENT_SECRET");
    let output = cmd
        .args(["setup", "plugin-hook", "--no-repair"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit for blocking failure; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let codes = blocking_failure_codes(&output);
    assert!(
        codes.contains(&"missing_oauth_client_secret".to_string()),
        "expected missing_oauth_client_secret in blocking_failures, got: {codes:?}"
    );
}

#[test]
fn oauth_missing_admin_email_produces_blocking_failure() {
    let dir = tempdir().unwrap();
    let mut cmd = oauth_setup_command(dir.path());
    cmd.env_remove("SOMA_MCP_AUTH_ADMIN_EMAIL");
    let output = cmd
        .args(["setup", "plugin-hook", "--no-repair"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit for blocking failure; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let codes = blocking_failure_codes(&output);
    assert!(
        codes.contains(&"missing_oauth_admin_email".to_string()),
        "expected missing_oauth_admin_email in blocking_failures, got: {codes:?}"
    );
}

// ── write_env OAuth branch (L28) ──────────────────────────────────────────────
//
// When `auth_mode = OAuth` with all OAuth fields set, `setup repair` must
// write a .env that includes SOMA_MCP_AUTH_MODE=oauth and all four OAuth
// credential lines.

#[test]
fn setup_repair_oauth_writes_oauth_env_lines() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("appdata");
    let mut cmd = oauth_setup_command(&data_dir);
    let output = cmd.args(["setup", "repair"]).output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_repair_success_or_windows_port_advisory(&json);
    assert_eq!(json["ran_repair"], true);

    let env_file = fs::read_to_string(data_dir.join(".env")).unwrap();
    assert!(
        env_file.contains("SOMA_MCP_AUTH_MODE=oauth"),
        ".env should contain SOMA_MCP_AUTH_MODE=oauth"
    );
    assert!(
        env_file.contains("SOMA_MCP_PUBLIC_URL=https://mcp.example.test"),
        ".env should contain SOMA_MCP_PUBLIC_URL"
    );
    assert!(
        env_file.contains("SOMA_MCP_GOOGLE_CLIENT_ID=test-client-id"),
        ".env should contain SOMA_MCP_GOOGLE_CLIENT_ID"
    );
    assert!(
        env_file.contains("SOMA_MCP_GOOGLE_CLIENT_SECRET=test-client-secret"),
        ".env should contain SOMA_MCP_GOOGLE_CLIENT_SECRET"
    );
    assert!(
        env_file.contains("SOMA_MCP_AUTH_ADMIN_EMAIL=admin@example.test"),
        ".env should contain SOMA_MCP_AUTH_ADMIN_EMAIL"
    );
    assert_env_file_mode(&data_dir.join(".env"));
}
