//! Quick-start example from README.md. Requires a real Incus daemon
//! reachable at the given socket path - run with:
//!
//!   cargo run -p incus-client --example basic -- /var/lib/incus/unix.socket
//!
//! This is compiled (via `cargo build --workspace --examples` in CI) but not
//! executed there, since it needs a live daemon - it exists to keep the
//! README's quick-start snippet honest, not as an integration test.
//!
//! `incus-client` itself compiles to an empty crate on non-Unix targets (see
//! `src/lib.rs`'s `#![cfg(unix)]`), so this example is gated the same way -
//! otherwise a Windows build would fail resolving `Client`/`ClientConfig`
//! from a library that doesn't export them on that platform.

#[cfg(unix)]
mod unix_main {
    use incus_client::{Client, ClientConfig};

    // current_thread: this crate's own [dependencies] only enable tokio's
    // `rt` feature (not `rt-multi-thread`), since a library shouldn't force
    // a runtime flavor on its consumers - this example only needs a runtime
    // to exist, so current_thread avoids adding an example-only
    // dev-dependency for it.
    #[tokio::main(flavor = "current_thread")]
    pub(crate) async fn run() -> incus_client::Result<()> {
        let socket_path = std::env::args()
            .nth(1)
            .unwrap_or_else(|| "/var/lib/incus/unix.socket".to_owned());
        let client = Client::new(ClientConfig::unix_socket(socket_path));

        // recursion = false: cheap, name/URL references only.
        let names = client.list_instances(false).await?;
        println!("{names:?}");

        // recursion = true: every instance's full object (config/devices/state).
        let full = client.list_instances(true).await?;
        println!("{full:?}");

        // Start an instance and wait for the operation to finish. `None`
        // waits indefinitely, transparently re-polling Incus's server-side
        // long-poll.
        let op = client.start_instance("web-01").await?;
        client.wait_for_operation(op.id, None).await?;

        Ok(())
    }
}

#[cfg(unix)]
fn main() -> incus_client::Result<()> {
    unix_main::run()
}

#[cfg(not(unix))]
fn main() {
    eprintln!("incus-client is Unix-only (see src/lib.rs); nothing to run here.");
}
