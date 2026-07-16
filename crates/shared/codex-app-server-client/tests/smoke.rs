//! Live integration test against the installed `codex` CLI. Skips (with a
//! printed message) instead of failing when `codex` isn't on PATH, so it's
//! safe to run in environments that don't have Codex installed - this test
//! exercises the real wire protocol, not a mock, which is the point of it.

use codex_app_server_client::protocol::{ClientInfo, InitializeParams};
use codex_app_server_client::{CodexAppServerClient, Error, Event};
use std::ffi::{OsStr, OsString};

struct EnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: impl AsRef<OsStr>) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            std::env::set_var(self.key, previous);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

#[tokio::test]
async fn handshake_and_no_auth_round_trip() {
    let tmp = tempfile::tempdir().expect("temporary Codex home should be created");
    let codex_home = tmp.path().join("codex-home");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&codex_home).expect("temporary CODEX_HOME should be created");
    std::fs::create_dir_all(&home).expect("temporary HOME should be created");
    let _codex_home = EnvVarGuard::set("CODEX_HOME", &codex_home);
    let _home = EnvVarGuard::set("HOME", &home);

    let (client, mut events) = match CodexAppServerClient::spawn("codex", &[]) {
        Ok(pair) => pair,
        Err(Error::Spawn { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping: `codex` is not on PATH");
            return;
        }
        Err(err) => panic!("failed to spawn codex app-server: {err}"),
    };

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            if let Event::Request(req) = event {
                let _ = req.respond_error(-32000, "no handler in smoke test", None);
            }
        }
    });

    let init = client
        .initialize(InitializeParams {
            client_info: ClientInfo {
                name: "codex_app_server_client_smoke_test".into(),
                title: None,
                version: env!("CARGO_PKG_VERSION").into(),
            },
            capabilities: None,
        })
        .await
        .expect("initialize should succeed against a freshly spawned app-server");
    assert!(!init.platform_os.is_empty());

    client
        .send_initialized()
        .expect("initialized notification should send on a live transport");

    // config/read requires no auth and no active thread - a good no-side-effect
    // check that a real typed round trip (request -> typed response) works.
    let config = client
        .config_read(serde_json::from_value(serde_json::json!({})).unwrap())
        .await
        .expect("config/read should succeed once initialized");
    // We don't assert on config contents (machine-dependent) - reaching this
    // point already proves handshake + one full typed request/response cycle.
    let _ = config;

    // A second call before completing correlates independently - regression
    // check for the request-id/pending-map bookkeeping.
    let features = client
        .experimental_feature_list(serde_json::from_value(serde_json::json!({})).unwrap())
        .await
        .expect("experimentalFeature/list should succeed");
    assert!(features.data.iter().all(|f| !f.name.is_empty()));
}
