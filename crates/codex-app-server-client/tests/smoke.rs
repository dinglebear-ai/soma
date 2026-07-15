//! Live integration test against the installed `codex` CLI. Skips (with a
//! printed message) instead of failing when `codex` isn't on PATH, so it's
//! safe to run in environments that don't have Codex installed - this test
//! exercises the real wire protocol, not a mock, which is the point of it.

use codex_app_server_client::protocol::{ClientInfo, InitializeParams};
use codex_app_server_client::{CodexAppServerClient, Error, Event};

#[tokio::test]
async fn handshake_and_no_auth_round_trip() {
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
                req.respond_error(-32000, "no handler in smoke test", None);
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
