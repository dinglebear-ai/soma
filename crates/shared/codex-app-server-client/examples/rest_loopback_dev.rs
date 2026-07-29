//! Posture 1 of 4: loopback dev — the "just let me try it" REST bridge.
//!
//! This is the lowest-friction way to poke at the REST adapter from your own
//! terminal. It mounts [`codex_app_server_client::rest::trusted_bridge_router`]
//! — the full raw callable/session bridge — with **zero authentication** and
//! binds to `127.0.0.1` by default.
//!
//! ## What this protects
//!
//! Exactly one thing: the OS loopback boundary. Binding to `127.0.0.1` means
//! only processes already running on this machine can open a TCP connection
//! to the port at all. That is the entire protection this example provides.
//!
//! ## What this does NOT protect
//!
//! Everything else. There is no token, no session cookie, no origin check,
//! nothing. Any process on this machine — any other user account able to
//! reach loopback, any browser tab running JavaScript against
//! `http://127.0.0.1:<port>`, any other container sharing this network
//! namespace — gets the same access this example grants a `curl` on the
//! command line: full, unauthenticated use of every route
//! `trusted_bridge_router()` exposes.
//!
//! If the bind address is ever changed away from loopback (a LAN IP,
//! `0.0.0.0`, a container bridge address reachable from sibling containers,
//! a cloud instance's private or public IP) this example does not stop you.
//! It only prints a loud warning. **Do not run this posture anywhere but
//! your own workstation.** For anything beyond a single trusted operator on
//! one machine, use `rest_bearer_auth` (add a token) or `rest_trusted_gateway`
//! (delegate auth to a real gateway) instead.
//!
//! ## What an attacker who reaches this port can do
//!
//! Everything the trusted bridge allows, as the OS user running this
//! process: create and delete Codex app-server sessions, drive turns to
//! completion, call any app-server JSON-RPC method via
//! `POST /v1/call/{method}` or `POST /v1/sessions/{sessionId}/call/{method}`,
//! and observe/answer server-originated requests via the event and
//! request-reply routes. Caller-chosen `command`/`extraArgs`/`config`
//! overrides and `approvalPolicy: "allow_all"` remain rejected here (unsafe
//! client options are off by default — see `rest_admin_unsafe` for that
//! separate, even more dangerous opt-in), but everything the *installed*
//! Codex binary can already do is fair game.
//!
//! ## When this posture is (and isn't) appropriate
//!
//! Appropriate: a single developer, on their own laptop or workstation,
//! experimenting with the REST bridge locally, for as long as the process is
//! running. Not appropriate: CI runners shared with other jobs, containers
//! reachable from sibling containers, multi-user hosts, anything with the
//! bind address overridden away from loopback, or anything left running
//! unattended.
//!
//! Run: `cargo run -p codex-app-server-client --features rest --example rest_loopback_dev`
//!
//! Try it:
//! ```text
//! curl http://127.0.0.1:43220/health
//! curl http://127.0.0.1:43220/v1/compatibility
//! ```

use std::net::SocketAddr;

use codex_app_server_client::rest;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = std::env::var("CODEX_APP_SERVER_REST_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:43220".to_owned())
        .parse::<SocketAddr>()?;

    warn_if_not_loopback(addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    println!("codex-app-server-client: rest_loopback_dev");
    println!("listening on http://{addr}");
    println!("posture: NO AUTH, full trusted bridge — loopback dev only, see module docs");
    println!();
    println!("  curl http://{addr}/health");
    println!("  curl http://{addr}/v1/compatibility");
    println!(
        "  curl -X POST http://{addr}/v1/call/config/read -H 'content-type: application/json' -d '{{\"params\":{{}}}}'"
    );
    println!();

    axum::serve(listener, rest::trusted_bridge_router()).await?;
    Ok(())
}

/// Prints a hard-to-miss warning if `addr` is not on the loopback interface.
/// Deliberately does not refuse to bind — this example's entire point is
/// "just let me try it" — but a silent bind to a reachable address would
/// defeat the one protection this posture actually has.
fn warn_if_not_loopback(addr: SocketAddr) {
    if addr.ip().is_loopback() {
        return;
    }
    eprintln!("================================================================");
    eprintln!("WARNING: rest_loopback_dev is binding to a NON-LOOPBACK address:");
    eprintln!("  {addr}");
    eprintln!();
    eprintln!("This example has NO AUTHENTICATION. Anything that can reach this");
    eprintln!("address gets full, unauthenticated access to the Codex REST bridge");
    eprintln!("(session creation, turn execution, raw JSON-RPC calls). This is");
    eprintln!("only meant for a single operator on loopback. Use rest_bearer_auth");
    eprintln!("or rest_trusted_gateway instead of overriding this example's bind");
    eprintln!("address.");
    eprintln!("================================================================");
}
