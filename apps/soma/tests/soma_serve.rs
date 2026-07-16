use std::{
    net::TcpListener,
    process::Stdio,
    time::{Duration, Instant},
};

use tempfile::tempdir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
};

#[tokio::test]
async fn soma_serve_starts_http_runtime() -> anyhow::Result<()> {
    let port = unused_loopback_port()?;
    let home = tempdir()?;
    let mut server = Command::new(env!("CARGO_BIN_EXE_soma"))
        .arg("serve")
        .env("HOME", home.path())
        .env("SOMA_HOME", home.path())
        .env("RUST_LOG", "warn")
        .env("SOMA_MCP_HOST", "127.0.0.1")
        .env("SOMA_MCP_PORT", port.to_string())
        .env("SOMA_MCP_NO_AUTH", "true")
        .env("SOMA_API_URL", "")
        .env("SOMA_API_KEY", "")
        .env("SOMA_MCP_TOKEN", "")
        .env_remove("SOMA_PROVIDER_DIR")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let health = wait_for_health(port).await;
    kill_child(&mut server).await;
    health
}

fn unused_loopback_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

async fn wait_for_health(port: u16) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let address = format!("127.0.0.1:{port}");
    loop {
        if Instant::now() > deadline {
            anyhow::bail!("soma serve on {address} did not become healthy");
        }
        if health_ok(&address).await.unwrap_or(false) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn health_ok(address: &str) -> anyhow::Result<bool> {
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    Ok(response.starts_with(b"HTTP/1.1 200") || response.starts_with(b"HTTP/1.0 200"))
}

async fn kill_child(child: &mut Child) {
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
}
