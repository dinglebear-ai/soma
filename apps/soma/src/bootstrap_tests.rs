#[cfg(feature = "mcp")]
use soma_application::{CodeModeExecuteRequest, ExecutionContext, SomaService};
#[cfg(feature = "mcp")]
use soma_client::SomaClient;
#[cfg(feature = "mcp")]
use soma_config::{McpConfig, SomaConfig};
#[cfg(feature = "mcp")]
use soma_domain::{AuthorizationMode, RequestId, Surface};
#[cfg(feature = "mcp")]
use soma_runtime::server::{AppState, AuthPolicy, empty_gateway_product_state};

#[cfg(feature = "mcp")]
use super::{authorization_mode, runtime_for_components};

#[cfg(feature = "mcp")]
fn state(auth_policy: AuthPolicy) -> AppState {
    let service = SomaService::new(
        SomaClient::new(&SomaConfig {
            api_url: String::new(),
            api_key: "test".into(),
            ..SomaConfig::default()
        })
        .expect("stub client should always build"),
    );
    let registry = soma_application::static_provider_registry(service.clone())
        .expect("static provider registry should always build");
    let runtime = runtime_for_components(service, registry, empty_gateway_product_state());
    AppState::new(
        McpConfig::default(),
        auth_policy,
        runtime,
        Default::default(),
    )
}

#[cfg(feature = "mcp")]
#[test]
fn maps_loopback_dev_policy_to_loopback_dev_mode() {
    let state = state(AuthPolicy::LoopbackDev);
    assert_eq!(authorization_mode(&state), AuthorizationMode::LoopbackDev);
}

#[cfg(feature = "mcp")]
#[test]
fn maps_trusted_gateway_policy_to_trusted_gateway_mode() {
    let state = state(AuthPolicy::TrustedGatewayUnscoped);
    assert_eq!(
        authorization_mode(&state),
        AuthorizationMode::TrustedGateway
    );
}

#[cfg(all(feature = "mcp", feature = "auth"))]
#[test]
fn maps_mounted_policy_to_mounted_mode() {
    let state = state(AuthPolicy::Mounted { auth_state: None });
    assert_eq!(authorization_mode(&state), AuthorizationMode::Mounted);
}

/// Reachability check for the PR 11 review fix: `runtime_for_components`
/// wires `soma_integrations::CodeModeApplicationPort` into `ApplicationPorts`
/// via `.with_codemode(...)`. Prove that wiring is live through the same
/// composition `apps/soma` actually uses (`state()` above), not just that
/// `CodeModeApplicationPort` works in its own crate's isolated unit tests.
/// Before the fix, `ApplicationPorts::unavailable()` left `codemode` on
/// `UnavailableEnginePort`, whose error code is always `"engine_unavailable"`
/// regardless of the request; asserting the code is something else (here,
/// `"codemode_disabled"`, since the wired port's default config is disabled)
/// proves a real `CodeModePort` is installed instead of the fallback.
#[cfg(feature = "mcp")]
#[tokio::test]
async fn codemode_port_is_wired_through_runtime_for_components_not_left_unavailable() {
    let state = state(AuthPolicy::LoopbackDev);
    let context = ExecutionContext::loopback(
        Surface::Mcp,
        RequestId::new("codemode-wiring-test").unwrap(),
    );
    let request = CodeModeExecuteRequest {
        source: "return 1;".to_owned(),
        input: serde_json::json!({}),
    };

    let error = state
        .application()
        .codemode_execute(request, context)
        .await
        .expect_err("default CodeModeApplicationPort config is disabled");

    assert_ne!(
        error.code, "engine_unavailable",
        "codemode port must not be the unwired UnavailableEnginePort fallback"
    );
    assert_eq!(error.code, "codemode_disabled");
}

#[cfg(feature = "cli")]
#[tokio::test]
async fn local_cli_composition_builds_the_application_catalog() {
    let providers = tempfile::tempdir().unwrap();
    std::fs::write(
        providers.path().join("fixture.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "fixture", "kind": "static-rust" },
          "tools": [{
            "name": "fixture_action",
            "description": "Composition refresh fixture",
            "input_schema": { "type": "object", "properties": {}, "additionalProperties": false },
            "output_schema": { "type": "object", "properties": {}, "additionalProperties": true },
            "cli": { "enabled": true, "command": "fixture" }
          }]
        }"#,
    )
    .unwrap();
    let application = super::cli_application_with_provider_dir(
        &soma_config::Config::default(),
        Some(providers.path()),
    )
    .await
    .unwrap();

    assert_eq!(application.resolve_cli_action("status").unwrap(), "status");
    assert_eq!(
        application.provider_for_action("status").as_deref(),
        Some("static-rust")
    );
    assert_eq!(
        application.resolve_cli_action("fixture").unwrap(),
        "fixture_action"
    );
}

#[cfg(feature = "cli")]
#[test]
fn python_runtime_composition_preserves_disabled_default_and_persistent_selection() {
    let default_runtime = super::python_provider_runtime(&soma_config::Config::default()).unwrap();
    let default_debug = format!("{default_runtime:?}");
    assert!(default_debug.contains("OneShot"));
    assert!(default_debug.contains("environment_preparer: false"));

    let mut config = soma_config::Config::default();
    config.python.mode = soma_config::PythonRunnerMode::Persistent;
    let persistent_runtime = super::python_provider_runtime(&config).unwrap();
    let persistent_debug = format!("{persistent_runtime:?}");
    assert!(persistent_debug.contains("Persistent"));
    assert!(persistent_debug.contains("environment_preparer: false"));
}

#[cfg(all(feature = "cli", unix))]
#[tokio::test]
async fn local_cli_composition_prepares_python_with_configured_immutable_environment() {
    use std::os::unix::fs::PermissionsExt;

    let temporary = tempfile::tempdir().unwrap();
    let providers = temporary.path().join("providers");
    let cache = temporary.path().join("cache");
    std::fs::create_dir(&providers).unwrap();
    std::fs::write(
        providers.join("configured.py"),
        r#"PROVIDER = {"name": "configured-python", "kind": "python"}

def configured_echo(value: str):
    return {"value": value}
"#,
    )
    .unwrap();
    let site_packages = temporary.path().join("site-packages");
    std::fs::create_dir(&site_packages).unwrap();
    std::fs::write(
        site_packages.join("configured_dependency.py"),
        "def decorate(value):\n    return f'configured:{value}'\n",
    )
    .unwrap();
    std::fs::write(
        providers.join("configured_dependency_provider.py"),
        r#"# /// script
# requires-python = ">=3.11"
# dependencies = ["configured-dependency>=1"]
# ///
from configured_dependency import decorate

PROVIDER = {"name": "configured-dependency-python", "kind": "python"}

def configured_dependency_echo(value: str):
    return {"value": decorate(value)}
"#,
    )
    .unwrap();
    let wheel = temporary
        .path()
        .join("soma_provider-0.2.0-cp38-abi3-manylinux_2_17_x86_64.whl");
    std::fs::write(&wheel, b"sdk wheel").unwrap();
    let python = std::process::Command::new("sh")
        .args(["-c", "command -v python3"])
        .output()
        .unwrap();
    assert!(python.status.success());
    let python = String::from_utf8(python.stdout).unwrap().trim().to_owned();
    let python_version = std::process::Command::new(&python)
        .args(["-c", "import platform; print(platform.python_version())"])
        .output()
        .unwrap();
    assert!(python_version.status.success());
    let python_version = String::from_utf8(python_version.stdout)
        .unwrap()
        .trim()
        .to_owned();
    let uv = temporary.path().join("fake-uv");
    let sdk_source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/python/python")
        .canonicalize()
        .unwrap();
    std::fs::write(
        &uv,
        format!(
            r#"#!/bin/sh
set -eu
case "$1" in
  --version)
    printf 'uv test-uv-1 (x86_64-unknown-linux-musl)\n'
    ;;
  lock)
    printf 'version = 1\n' > uv.lock
    ;;
  sync)
    mkdir -p .venv/bin
    cat > .venv/bin/python <<'PYTHON'
#!/bin/sh
set -eu
if [ "$1" = "-I" ] && [ "$2" = "-c" ]; then
  code="$3"
  shift 3
  exec "{python}" -I -c 'import sys; sys.path.insert(0, r"{site_packages}");'"$code" "$@"
fi
if [ "$1" = "-I" ] && [ "$2" = "-m" ] && [ "$3" = "soma_provider.runner" ]; then
  exec "{python}" -I -c 'import runpy,sys; sys.path[:0] = [r"{sdk_source}", r"{site_packages}"]; runpy.run_module("soma_provider.runner", run_name="__main__")'
fi
if [ "$1" = "-c" ]; then
  code="$2"
  shift 2
  exec "{python}" -c 'import sys; sys.path.insert(0, r"{site_packages}");'"$code" "$@"
fi
exec "{python}" "$@"
PYTHON
    chmod 755 .venv/bin/python
    ;;
  pip)
    ;;
  *)
    exit 64
    ;;
esac
"#,
            site_packages = site_packages.display(),
            sdk_source = sdk_source.display(),
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&uv).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&uv, permissions).unwrap();

    let mut config = soma_config::Config::default();
    config.python.environment = soma_config::PythonEnvironmentConfig {
        enabled: true,
        cache_root: cache.display().to_string(),
        uv_program: uv.display().to_string(),
        uv_version: "test-uv-1".to_owned(),
        python_executable: python,
        runtime_implementation: "cpython".to_owned(),
        runtime_version: python_version,
        runtime_platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        wheel_platform_tag: "manylinux_2_17_x86_64".to_owned(),
        sdk_wheel: wheel.display().to_string(),
        sdk_wheel_sha256: "05b77fc1f7be217ecc4ab4b2cf83220ef7c023d63b799d67c41685706b8d3b30"
            .to_owned(),
        offline: false,
        update: false,
        policy_version: soma_application::ENVIRONMENT_PLAN_VERSION,
    };

    let one_shot = super::cli_application_with_provider_dir(&config, Some(&providers))
        .await
        .expect("production composition should prepare and activate Python");
    assert_eq!(
        one_shot.provider_for_action("configured_echo").as_deref(),
        Some("configured-python")
    );
    assert_eq!(
        one_shot
            .provider_for_action("configured_dependency_echo")
            .as_deref(),
        Some("configured-dependency-python")
    );
    assert!(
        std::fs::read_dir(cache.join("python/v2"))
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.path().join("soma-environment.json").is_file()),
        "production composition should publish a ready immutable cache"
    );
    let context = |id: &str| {
        soma_application::ExecutionContext::loopback(
            soma_domain::Surface::Cli,
            soma_domain::RequestId::new(id).unwrap(),
        )
    };
    let output = one_shot
        .execute_action(
            soma_application::ExecuteActionRequest {
                action: "configured_dependency_echo".to_owned(),
                params: serde_json::json!({"value": "one-shot"}),
            },
            context("configured-python-one-shot"),
        )
        .await
        .expect("prepared one-shot provider should execute");
    assert_eq!(output.output["value"], "configured:one-shot");

    config.python.mode = soma_config::PythonRunnerMode::Persistent;
    let persistent = super::cli_application_with_provider_dir(&config, Some(&providers))
        .await
        .expect("production composition should start a prepared persistent provider");
    let output = persistent
        .execute_action(
            soma_application::ExecuteActionRequest {
                action: "configured_dependency_echo".to_owned(),
                params: serde_json::json!({"value": "persistent"}),
            },
            context("configured-python-persistent"),
        )
        .await
        .expect("prepared persistent provider should execute");
    assert_eq!(output.output["value"], "configured:persistent");

    let unavailable_uv = temporary.path().join("fake-uv-unavailable");
    std::fs::rename(&uv, &unavailable_uv).unwrap();
    std::fs::write(
        providers.join("configured_dependency_provider.py"),
        r#"# /// script
# requires-python = ">=3.11"
# dependencies = ["configured-dependency>=2"]
# ///
from configured_dependency import decorate
PROVIDER = {"name": "configured-dependency-python", "kind": "python"}
def configured_dependency_echo(value: str):
    return {"value": decorate(value)}
"#,
    )
    .unwrap();
    let healthy_snapshot = persistent.catalog_snapshot().id;
    let retained_snapshot = persistent
        .refresh_providers_async()
        .await
        .expect("environment preparation failure should preserve the active registry");
    assert_eq!(retained_snapshot.id, healthy_snapshot);
    let output = persistent
        .execute_action(
            soma_application::ExecuteActionRequest {
                action: "configured_dependency_echo".to_owned(),
                params: serde_json::json!({"value": "retained"}),
            },
            context("configured-python-retained"),
        )
        .await
        .expect("failed environment preparation must retain the healthy generation");
    assert_eq!(output.output["value"], "configured:retained");

    std::fs::rename(&unavailable_uv, &uv).unwrap();
    std::fs::write(
        providers.join("configured_dependency_provider.py"),
        r#"# /// script
# requires-python = ">=3.11"
# dependencies = ["configured-dependency>=1"]
# ///
from configured_dependency import decorate
PROVIDER = {"name": "configured-dependency-python", "kind": "python"}
def configured_dependency_echo(value: str):
    return {"value": decorate(value)}
"#,
    )
    .unwrap();
    let mut invalid_identity = config.clone();
    invalid_identity.python.environment.uv_version = "wrong-version".to_owned();
    let error =
        match super::cli_application_with_provider_dir(&invalid_identity, Some(&providers)).await {
            Ok(_) => panic!("a false uv identity must fail before registry construction"),
            Err(error) => error,
        };
    assert!(error.to_string().contains("uv identity mismatch"));

    std::fs::remove_file(&uv).unwrap();
    config.python.environment.offline = true;
    let restarted = super::cli_application_with_provider_dir(&config, Some(&providers))
        .await
        .expect("offline production restart should reuse the complete cache without uv");
    assert_eq!(
        restarted
            .provider_for_action("configured_dependency_echo")
            .as_deref(),
        Some("configured-dependency-python")
    );
}

#[cfg(feature = "auth")]
#[test]
fn soma_auth_config_builder_supports_upstream_oauth_without_inbound_oauth() {
    let cfg = soma_integrations::auth::soma_auth_config_builder()
        .build_from_sources([
            (
                "SOMA_MCP_PUBLIC_URL".to_string(),
                "https://mcp.example.com".to_string(),
            ),
            (
                "SOMA_MCP_AUTH_SQLITE_PATH".to_string(),
                "/tmp/soma-auth.db".to_string(),
            ),
        ])
        .unwrap();

    assert!(matches!(cfg.mode, soma_auth::config::AuthMode::Bearer));
    assert_eq!(cfg.resource_path, "/mcp");
    assert!(cfg.scopes_supported.contains(&"soma:admin".to_string()));
}
