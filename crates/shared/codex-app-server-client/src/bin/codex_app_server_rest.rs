//! Standalone binary that runs the `rest` feature's HTTP adapter around a
//! local `codex app-server` process.
//!
//! ```sh
//! cargo install --path crates/shared/codex-app-server-client --features rest
//! codex-app-server-rest --host 127.0.0.1 --port 43210 --mode text-turn
//! ```
//!
//! This is a thin process wrapper: every actual routing/backend decision
//! lives in [`codex_app_server_client::rest`]. What this file owns is
//! argument/env resolution and the safety refusals described in its own
//! doc comments below - see [`resolve_config`] and [`check_safety`].
//!
//! # Argument parsing: hand-rolled, not `clap`
//!
//! `clap` is not a dependency of this crate, and this file does not add it.
//! `codex-app-server-client` advertises "zero path-dependencies, minimal
//! crates.io footprint" as a feature (see README.md and this crate's
//! `Cargo.toml` comments) - every existing binary in the `soma` workspace
//! that needs CLI parsing (`apps/soma/src/bin/soma.rs`,
//! `crates/soma/cli/src/lib.rs`) already hand-rolls it rather than pulling
//! in `clap`, so a `clap` dependency here would be both a footprint
//! regression for this specific crate's stated design goal *and* a house-style
//! outlier. The actual argument grammar here is small and fixed (five named
//! flags, all `--flag value` or `--flag=value`, no subcommands, no
//! positional arguments, no combinable short flags) - well within what a
//! small hand-rolled loop can parse correctly and exhaustively unit-test,
//! which is the real risk `clap` mitigates for larger/more dynamic grammars.
//! [`parse_cli`] is that loop; `tests::cli` exercises both `--flag value`
//! and `--flag=value` forms, unknown-flag rejection, and duplicate-flag
//! rejection.

use std::{
    net::{IpAddr, SocketAddr},
    process::ExitCode,
};

use codex_app_server_client::rest::{self, RestLimits, RestRouterOptions};

const USAGE: &str = "\
codex-app-server-rest - HTTP adapter around a local `codex app-server` process

USAGE:
    codex-app-server-rest [OPTIONS]

OPTIONS:
    --host <HOST>                    Bind host (default: 127.0.0.1)
                                      [env: CODEX_APP_SERVER_REST_HOST]
    --port <PORT>                    Bind port (default: 43210)
                                      [env: CODEX_APP_SERVER_REST_PORT]
    --mode <MODE>                    text-turn | trusted-bridge | health-only
                                      (default: text-turn)
                                      [env: CODEX_APP_SERVER_REST_MODE]
    --token <TOKEN>                  Require `Authorization: Bearer <TOKEN>`
                                      on every request (except /health,
                                      /v1/health) via rest::bearer_auth.
                                      [env: CODEX_APP_SERVER_REST_TOKEN]
    --allow-unsafe-client-options    ADMIN-ONLY. Lets REST callers override
                                      the `codex` command to run, pass it
                                      extra CLI args, set arbitrary config,
                                      and request approvalPolicy: allow_all.
                                      Requires --token or a loopback bind.
    -h, --help                       Print this help and exit.
    -V, --version                    Print the version and exit.

Also honors the pre-existing CODEX_APP_SERVER_REST_ADDR=host:port used by
`examples/rest_server.rs`, as a combined host+port fallback below the
per-field env vars above and above the built-in defaults - see
`resolve_config`'s doc comment for the exact precedence order.

Resource limits (max sessions, call concurrency, timeouts, ...) are not
binary flags - they come entirely from the CODEX_APP_SERVER_REST_* variables
documented on `codex_app_server_client::rest::RestLimits`. A malformed limit
env var exits non-zero immediately, before any socket is bound.

MODES:
    text-turn        Mounts /health, /v1/health, /v1/compatibility, and
                      POST /v1/text-turn only. No raw session/call bridge.
    trusted-bridge    Everything text-turn mounts, plus the full raw
                      session/call/event bridge (POST /v1/sessions, ...).
                      Exposes every codex app-server method to REST callers.
                      Binding this non-loopback requires --token.
    health-only       Mounts only /health, /v1/health, /v1/compatibility.
                      Mounts no executing route at all.

Every route's exact behavior, request/response shape, and gating is
documented in this crate's generated OpenAPI spec
(`codex_app_server_client::rest::openapi_spec()`, checked in at
crates/shared/codex-app-server-client/openapi.json) and in README.md.
";

// Multi-threaded runtime (tokio's default `#[tokio::main]`): this is a real
// server that can hold up to `RestLimits::max_sessions` concurrent sessions,
// each driving its own `codex app-server` child. On a single-threaded runtime
// one busy connection starves every other session sharing the process (flagged
// in review); a worker pool lets them make progress in parallel. The
// single-session `examples/rest_*.rs` deliberately stay `current_thread` -
// they demo one session and have no such contention.
#[tokio::main]
async fn main() -> ExitCode {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    run(raw_args, &|key| std::env::var(key).ok()).await
}

/// The real `main` body, minus process-global argument/env access - takes
/// both as parameters so behavior above the actual socket bind is exercised
/// by [`resolve_config`]/[`check_safety`]'s unit tests without touching
/// `std::env` or the network. Only this function and everything it calls
/// after the `bind_and_serve` boundary does real I/O.
async fn run(raw_args: Vec<String>, env: &dyn Fn(&str) -> Option<String>) -> ExitCode {
    let flags = match parse_cli(raw_args) {
        Ok(flags) => flags,
        Err(error) => return fail(&error),
    };
    if flags.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if flags.version {
        println!("codex-app-server-rest {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    let config = match resolve_config(&flags, env) {
        Ok(config) => config,
        Err(error) => return fail(&error),
    };
    let limits = match RestLimits::try_from_env() {
        Ok(limits) => limits,
        Err(error) => return fail(&error.to_string()),
    };
    let safety = match check_safety(&config) {
        Ok(safety) => safety,
        Err(error) => return fail(&error),
    };
    for warning in &safety.warnings {
        eprintln!("codex-app-server-rest: warning: {warning}");
    }

    let addr = match format!("{}:{}", config.host, config.port).parse::<SocketAddr>() {
        Ok(addr) => addr,
        Err(error) => {
            return fail(&format!(
                "invalid bind address `{}:{}`: {error}",
                config.host, config.port
            ));
        }
    };

    print_effective_config(&config, &limits, addr);
    bind_and_serve(addr, build_router(&config, limits)).await
}

fn fail(message: &str) -> ExitCode {
    eprintln!("codex-app-server-rest: {message}");
    ExitCode::FAILURE
}

/// Binds `addr` and serves `router` until the process is killed or the
/// server errors. Isolated from [`run`] so every decision made *before* a
/// real socket touches the network stays testable without one.
async fn bind_and_serve(addr: SocketAddr, router: axum::Router) -> ExitCode {
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(error) => return fail(&format!("failed to bind {addr}: {error}")),
    };
    println!("codex-app-server-rest listening on http://{addr}");
    match axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(&format!("server error: {error}")),
    }
}

/// Resolves when the process is asked to stop, so [`bind_and_serve`] can drain
/// in-flight requests instead of dropping them mid-flight.
///
/// Without this, a `SIGTERM` (systemd stop, `docker stop`, an orchestrator
/// rolling the pod) kills the process instantly, tearing down every active
/// session and orphaning the `codex app-server` children they own. Graceful
/// shutdown stops accepting new connections and lets the ones in progress
/// finish first.
///
/// Waits on `ctrl-c` on every platform, plus `SIGTERM` on unix (the signal a
/// service manager actually sends); whichever arrives first wins. A failure to
/// install a handler is treated as "never fires" rather than a crash - losing
/// graceful shutdown is not worth taking the server down over.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    println!("codex-app-server-rest: shutdown signal received, draining");
}

fn build_router(config: &ResolvedConfig, limits: RestLimits) -> axum::Router {
    let options = match config.mode {
        Mode::TextTurn => RestRouterOptions::text_turn(),
        Mode::TrustedBridge => RestRouterOptions::trusted_bridge(),
        Mode::HealthOnly => RestRouterOptions::default(),
    }
    .with_unsafe_client_options(config.allow_unsafe_client_options)
    .with_limits(limits);

    let router = rest::router_with_options(options);
    match &config.token {
        Some(token) => router.layer(rest::bearer_auth(token.clone())),
        None => router,
    }
}

/// Prints the effective startup configuration. Deliberately never prints
/// `config.token` itself - only whether one is set - so a pasted terminal
/// log or CI output never leaks the secret this process was just handed.
fn print_effective_config(config: &ResolvedConfig, limits: &RestLimits, addr: SocketAddr) {
    println!("codex-app-server-rest starting:");
    println!("  mode:                  {}", config.mode.as_str());
    println!("  bind:                  http://{addr}");
    println!(
        "  auth:                  {}",
        if config.token.is_some() {
            "bearer token required (except /health, /v1/health)"
        } else {
            "disabled (no --token / CODEX_APP_SERVER_REST_TOKEN set)"
        }
    );
    println!(
        "  unsafe client options: {}",
        if config.allow_unsafe_client_options {
            "ENABLED"
        } else {
            "disabled"
        }
    );
    // Printed via `RestLimits`'s `Debug` derive rather than a hand-listed
    // format string. The point of this banner is to show an operator what they
    // actually got, so a field it forgets to mention is worse than useless -
    // and a hand-written list forgets by default: it silently omits every
    // field added to `RestLimits` after it was written (which is exactly what
    // happened to `min_stream_poll_timeout` and `events_channel_capacity`).
    // The derive cannot drift.
    println!("  limits:                {limits:?}");
}

/// Router mode selected by `--mode`/`CODEX_APP_SERVER_REST_MODE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    TextTurn,
    TrustedBridge,
    HealthOnly,
}

impl Mode {
    const DEFAULT_RAW: &'static str = "text-turn";

    fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "text-turn" => Ok(Self::TextTurn),
            "trusted-bridge" => Ok(Self::TrustedBridge),
            "health-only" => Ok(Self::HealthOnly),
            other => Err(format!(
                "invalid --mode `{other}`: expected one of `text-turn`, `trusted-bridge`, `health-only`"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::TextTurn => "text-turn",
            Self::TrustedBridge => "trusted-bridge",
            Self::HealthOnly => "health-only",
        }
    }
}

/// Raw, unvalidated flag values as parsed from argv - see [`parse_cli`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct CliFlags {
    host: Option<String>,
    port: Option<String>,
    mode: Option<String>,
    token: Option<String>,
    allow_unsafe_client_options: bool,
    help: bool,
    version: bool,
}

/// Parses argv (excluding `argv[0]`) into [`CliFlags`]. Accepts both
/// `--flag value` and `--flag=value` for every value-taking flag, rejects
/// any unrecognized flag, and rejects a flag supplied more than once - all
/// loudly (`Err`, never a silent last-write-wins), matching the parsing
/// posture the rest of this workspace's hand-rolled CLI parsers use (see
/// `crates/soma/cli/src/lib.rs`).
fn parse_cli<I, S>(args: I) -> Result<CliFlags, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let mut flags = CliFlags::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let (name, inline_value) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value.to_owned())),
            _ => (arg, None),
        };

        match name {
            "--help" | "-h" => {
                reject_inline_value(name, &inline_value)?;
                flags.help = true;
                index += 1;
            }
            "--version" | "-V" => {
                reject_inline_value(name, &inline_value)?;
                flags.version = true;
                index += 1;
            }
            "--allow-unsafe-client-options" => {
                reject_inline_value(name, &inline_value)?;
                if flags.allow_unsafe_client_options {
                    return Err(format!("duplicate {name}"));
                }
                flags.allow_unsafe_client_options = true;
                index += 1;
            }
            "--host" | "--port" | "--mode" | "--token" => {
                let slot = match name {
                    "--host" => &mut flags.host,
                    "--port" => &mut flags.port,
                    "--mode" => &mut flags.mode,
                    "--token" => &mut flags.token,
                    _ => unreachable!("matched above"),
                };
                if slot.is_some() {
                    return Err(format!("duplicate {name}"));
                }
                // `--flag=value` consumes one argv slot (the value was already
                // inline in `args[index]`); `--flag value` consumes two (this
                // one plus the next). `inline_value.is_some()` already carries
                // that distinction - no need to re-derive it.
                let (value, consumed) = match inline_value {
                    Some(value) => (value, 1),
                    None => {
                        let next = args
                            .get(index + 1)
                            .ok_or_else(|| format!("{name} requires a value"))?;
                        if next.starts_with("--") {
                            return Err(format!("{name} requires a value"));
                        }
                        (next.clone(), 2)
                    }
                };
                *slot = Some(value);
                index += consumed;
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok(flags)
}

fn reject_inline_value(name: &str, inline_value: &Option<String>) -> Result<(), String> {
    if inline_value.is_some() {
        Err(format!("{name} does not take a value"))
    } else {
        Ok(())
    }
}

/// Fully resolved, validated configuration - the output of merging
/// [`CliFlags`] with environment fallbacks.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedConfig {
    host: String,
    port: u16,
    mode: Mode,
    /// `None` when unset; a token that is present but blank/whitespace-only
    /// is treated as unset (see [`resolve_config`]) rather than passed to
    /// [`rest::bearer_auth`], which panics on a blank token - a misconfigured
    /// empty env var should read as "no auth configured", not crash the process.
    token: Option<String>,
    allow_unsafe_client_options: bool,
}

/// Merges [`CliFlags`] with environment fallbacks into a [`ResolvedConfig`],
/// with **flag > per-field env var > `CODEX_APP_SERVER_REST_ADDR` >
/// built-in default** precedence for `host`/`port`.
///
/// `CODEX_APP_SERVER_REST_ADDR` is the pre-existing `host:port` variable
/// `examples/rest_server.rs` already reads (see this crate's README.md).
/// It is honored here for continuity, but ranked below the newer per-field
/// `CODEX_APP_SERVER_REST_HOST`/`_PORT` variables introduced alongside this
/// binary: the per-field variables can express "override just the port,
/// keep the default host" (or vice versa), which a single combined `ADDR`
/// variable cannot, so the more expressive newer variables should win when
/// both are set. `ADDR` is parsed as a single `host:port` pair - if it's
/// set but fails to parse as a [`SocketAddr`], that's silently ignored
/// (falls through to the built-in default) rather than a hard error, since
/// unlike [`RestLimits::try_from_env`]'s variables this one is a
/// continuity/back-compat fallback, not this binary's primary configuration
/// surface; treating a malformed *primary* `--host`/`--port` as a hard
/// error (below) is what actually matters for "don't silently ship a wrong
/// value".
fn resolve_config(
    flags: &CliFlags,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<ResolvedConfig, String> {
    let addr_fallback =
        env("CODEX_APP_SERVER_REST_ADDR").and_then(|raw| raw.parse::<SocketAddr>().ok());

    let host = flags
        .host
        .clone()
        .or_else(|| env("CODEX_APP_SERVER_REST_HOST"))
        .or_else(|| addr_fallback.map(|addr| addr.ip().to_string()))
        .unwrap_or_else(|| "127.0.0.1".to_owned());

    let port = match flags
        .port
        .clone()
        .or_else(|| env("CODEX_APP_SERVER_REST_PORT"))
    {
        Some(raw) => raw
            .trim()
            .parse::<u16>()
            .map_err(|_| format!("invalid --port `{raw}`: expected an integer 0-65535"))?,
        None => addr_fallback.map(|addr| addr.port()).unwrap_or(43210),
    };

    let mode_raw = flags
        .mode
        .clone()
        .or_else(|| env("CODEX_APP_SERVER_REST_MODE"))
        .unwrap_or_else(|| Mode::DEFAULT_RAW.to_owned());
    let mode = Mode::parse(&mode_raw)?;

    let token = flags
        .token
        .clone()
        .or_else(|| env("CODEX_APP_SERVER_REST_TOKEN"))
        .filter(|token| !token.trim().is_empty());

    Ok(ResolvedConfig {
        host,
        port,
        mode,
        token,
        allow_unsafe_client_options: flags.allow_unsafe_client_options,
    })
}

/// `true` for `127.0.0.0/8`, `::1`, and the literal hostname `localhost`
/// (case-insensitive), the same three shapes
/// `soma-contracts::McpConfig::is_loopback` treats as loopback for the main
/// `soma` binary's own auth-policy selection (see that crate's docs). This
/// crate cannot depend on it directly (zero workspace path-dependencies, see
/// README.md), so the check is reimplemented here rather than shared.
/// Anything else is conservatively treated as non-loopback, including a
/// hostname that *might* resolve to loopback (`0.0.0.0`, arbitrary DNS
/// names, IPv6 forms not accepted by [`IpAddr`]'s `FromStr`): the cost of a
/// false "not loopback" is an operator being asked to also pass `--token`,
/// which is always safe to do, while the cost of a false "is loopback" is a
/// network-facing bridge with no auth, which is exactly the outcome the
/// safety refusal in [`check_safety`] exists to prevent.
fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Non-fatal operator warnings produced by [`check_safety`] alongside a
/// passing (`Ok`) configuration - printed by [`run`], never suppressed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SafetyOutcome {
    warnings: Vec<String>,
}

/// The safety gate this binary exists to enforce - see its module doc
/// comment. Two independent checks, both structured the same way: refuse
/// outright when the risky combination has no mitigating factor present,
/// warn (not refuse) when it does.
///
/// First, `--mode trusted-bridge` on a non-loopback bind with no `--token`
/// would put the full raw session/call/event bridge on the network with
/// zero authorization, so that combination is refused. The same combination
/// on a loopback bind is the intended local-dev path and is allowed, but
/// still gets a warning, since any other local user/process on the machine
/// can drive it.
///
/// Second, `--allow-unsafe-client-options` on a non-loopback bind with no
/// `--token` would let *any* network caller choose the `codex` executable
/// to run and its arguments, i.e. remote command execution, so that
/// combination is refused too. Loopback or a token present downgrades this
/// to a loud warning describing exactly what the flag permits, since it's
/// real functionality some deployments genuinely want (an admin-only
/// sidecar that needs `command`/`extraArgs`/`config` overrides), not a
/// mistake to always block.
fn check_safety(config: &ResolvedConfig) -> Result<SafetyOutcome, String> {
    let loopback = is_loopback_host(&config.host);
    let has_token = config.token.is_some();
    let mut warnings = Vec::new();

    if config.mode == Mode::TrustedBridge {
        if !loopback && !has_token {
            return Err(format!(
                "refusing to bind {}:{} in --mode trusted-bridge without --token: the trusted \
                 bridge exposes raw session/call/event routes with no built-in authorization, so \
                 a non-loopback bind with no token would let any network peer drive `codex \
                 app-server` as this process. Pass --token <secret> (requires \
                 `Authorization: Bearer <secret>` on every request), or bind a loopback host \
                 (127.0.0.1, ::1, localhost) instead.",
                config.host, config.port
            ));
        }
        if !has_token {
            warnings.push(format!(
                "no --token set: {}:{} is loopback-only so this is fine for local development, \
                 but any other local process/user on this machine can drive the trusted bridge \
                 unauthenticated.",
                config.host, config.port
            ));
        }
    }

    if config.allow_unsafe_client_options {
        if !loopback && !has_token {
            return Err(
                "refusing --allow-unsafe-client-options on a non-loopback bind without --token: \
                 this flag lets REST callers override the `codex` command to execute, pass it \
                 extra CLI arguments, and set arbitrary app-server config - i.e. arbitrary host \
                 command execution for any network caller. Pass --token <secret> or bind a \
                 loopback host instead."
                    .to_owned(),
            );
        }
        warnings.push(
            "--allow-unsafe-client-options is enabled: REST callers may override the `codex` \
             command to execute, pass it extra CLI arguments, set arbitrary app-server config \
             overrides, and request approvalPolicy: \"allow_all\". This weakens or bypasses \
             sandboxing that would otherwise apply - only enable this for a fully trusted, \
             admin-only caller set."
                .to_owned(),
        );
    }

    Ok(SafetyOutcome { warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The startup banner must name every limit, so an operator can see what
    /// they actually got rather than what they hoped for.
    ///
    /// Pinned because the banner used to hand-list the fields and silently
    /// dropped two that were added to `RestLimits` later. It renders via the
    /// `Debug` derive now, which can't drift - this test fails if someone
    /// replaces that with a hand-written list again, or adds a field whose
    /// `Debug` output is suppressed.
    #[test]
    fn effective_config_banner_names_every_limits_field() {
        let rendered = format!("{:?}", RestLimits::default());
        for field in [
            "max_sessions",
            "max_one_shot_concurrency",
            "max_session_call_concurrency",
            "max_session_call_concurrency_per_session",
            "max_poll_timeout",
            "min_stream_poll_timeout",
            "max_text_turn_duration",
            "max_text_turn_output_bytes",
            "pending_request_ttl",
            "max_pending_requests_per_session",
            "events_channel_capacity",
            "idle_session_ttl",
            "compatibility_ttl",
            "sse_keep_alive_interval",
        ] {
            assert!(
                rendered.contains(field),
                "the startup banner renders RestLimits via Debug, but `{field}` is missing from \
                 that output: {rendered}"
            );
        }
    }

    fn env_map(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        move |key: &str| {
            pairs
                .iter()
                .find(|(candidate, _)| candidate == key)
                .map(|(_, value)| value.clone())
        }
    }

    fn no_env() -> impl Fn(&str) -> Option<String> {
        env_map(&[])
    }

    fn strs(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    mod cli {
        use super::*;

        #[test]
        fn parses_separate_value_flags() {
            let flags = parse_cli(strs(&["--host", "0.0.0.0", "--port", "9000"])).unwrap();
            assert_eq!(flags.host.as_deref(), Some("0.0.0.0"));
            assert_eq!(flags.port.as_deref(), Some("9000"));
        }

        #[test]
        fn parses_inline_equals_value_flags() {
            let flags = parse_cli(strs(&["--host=0.0.0.0", "--port=9000"])).unwrap();
            assert_eq!(flags.host.as_deref(), Some("0.0.0.0"));
            assert_eq!(flags.port.as_deref(), Some("9000"));
        }

        #[test]
        fn parses_mixed_forms_in_one_invocation() {
            let flags = parse_cli(strs(&[
                "--host=0.0.0.0",
                "--port",
                "9000",
                "--mode=health-only",
            ]))
            .unwrap();
            assert_eq!(flags.host.as_deref(), Some("0.0.0.0"));
            assert_eq!(flags.port.as_deref(), Some("9000"));
            assert_eq!(flags.mode.as_deref(), Some("health-only"));
        }

        #[test]
        fn parses_bool_and_help_and_version_flags() {
            let flags = parse_cli(strs(&["--allow-unsafe-client-options"])).unwrap();
            assert!(flags.allow_unsafe_client_options);

            assert!(parse_cli(strs(&["--help"])).unwrap().help);
            assert!(parse_cli(strs(&["-h"])).unwrap().help);
            assert!(parse_cli(strs(&["--version"])).unwrap().version);
            assert!(parse_cli(strs(&["-V"])).unwrap().version);
        }

        #[test]
        fn rejects_unknown_flags() {
            let error = parse_cli(strs(&["--bogus"])).unwrap_err();
            assert!(error.contains("unknown argument"), "{error}");
            assert!(error.contains("--bogus"), "{error}");
        }

        #[test]
        fn rejects_bool_flag_with_inline_value() {
            let error = parse_cli(strs(&["--allow-unsafe-client-options=true"])).unwrap_err();
            assert!(error.contains("does not take a value"), "{error}");
        }

        #[test]
        fn rejects_value_flag_missing_its_value_at_end_of_args() {
            let error = parse_cli(strs(&["--host"])).unwrap_err();
            assert!(error.contains("--host requires a value"), "{error}");
        }

        #[test]
        fn rejects_value_flag_immediately_followed_by_another_flag() {
            let error = parse_cli(strs(&["--host", "--port", "9000"])).unwrap_err();
            assert!(error.contains("--host requires a value"), "{error}");
        }

        #[test]
        fn rejects_duplicate_value_flags() {
            let error = parse_cli(strs(&["--host", "a", "--host", "b"])).unwrap_err();
            assert!(error.contains("duplicate --host"), "{error}");
        }

        #[test]
        fn rejects_duplicate_bool_flags() {
            let error = parse_cli(strs(&[
                "--allow-unsafe-client-options",
                "--allow-unsafe-client-options",
            ]))
            .unwrap_err();
            assert!(
                error.contains("duplicate --allow-unsafe-client-options"),
                "{error}"
            );
        }

        #[test]
        fn accepts_a_single_dash_value_that_is_not_a_recognized_short_flag() {
            // A value flag only rejects its next token when that token starts
            // with `--` (see `parse_cli`'s value-flag arm) - a single-leading-dash
            // token like `-not-actually-a-flag` is accepted as the value, not
            // mistaken for a flag, since this grammar has no single-dash flags
            // other than the exact tokens `-h`/`-V`.
            let flags = parse_cli(strs(&["--token", "-not-actually-a-flag"])).unwrap();
            assert_eq!(flags.token.as_deref(), Some("-not-actually-a-flag"));
        }
    }

    mod config {
        use super::*;

        #[test]
        fn defaults_when_nothing_is_set() {
            let config = resolve_config(&CliFlags::default(), &no_env()).unwrap();
            assert_eq!(config.host, "127.0.0.1");
            assert_eq!(config.port, 43210);
            assert_eq!(config.mode, Mode::TextTurn);
            assert_eq!(config.token, None);
            assert!(!config.allow_unsafe_client_options);
        }

        #[test]
        fn flags_win_over_env() {
            let flags = CliFlags {
                host: Some("10.0.0.1".to_owned()),
                port: Some("1111".to_owned()),
                mode: Some("trusted-bridge".to_owned()),
                token: Some("flag-token".to_owned()),
                ..CliFlags::default()
            };
            let env = env_map(&[
                ("CODEX_APP_SERVER_REST_HOST", "192.168.0.1"),
                ("CODEX_APP_SERVER_REST_PORT", "2222"),
                ("CODEX_APP_SERVER_REST_MODE", "health-only"),
                ("CODEX_APP_SERVER_REST_TOKEN", "env-token"),
            ]);
            let config = resolve_config(&flags, &env).unwrap();
            assert_eq!(config.host, "10.0.0.1");
            assert_eq!(config.port, 1111);
            assert_eq!(config.mode, Mode::TrustedBridge);
            assert_eq!(config.token.as_deref(), Some("flag-token"));
        }

        #[test]
        fn per_field_env_wins_when_no_flag_is_set() {
            let env = env_map(&[
                ("CODEX_APP_SERVER_REST_HOST", "192.168.0.1"),
                ("CODEX_APP_SERVER_REST_PORT", "2222"),
            ]);
            let config = resolve_config(&CliFlags::default(), &env).unwrap();
            assert_eq!(config.host, "192.168.0.1");
            assert_eq!(config.port, 2222);
        }

        #[test]
        fn addr_env_fills_host_and_port_below_per_field_env() {
            let env = env_map(&[("CODEX_APP_SERVER_REST_ADDR", "203.0.113.5:9999")]);
            let config = resolve_config(&CliFlags::default(), &env).unwrap();
            assert_eq!(config.host, "203.0.113.5");
            assert_eq!(config.port, 9999);
        }

        #[test]
        fn per_field_port_env_overrides_addr_env_host_still_from_addr() {
            let env = env_map(&[
                ("CODEX_APP_SERVER_REST_ADDR", "203.0.113.5:9999"),
                ("CODEX_APP_SERVER_REST_PORT", "1234"),
            ]);
            let config = resolve_config(&CliFlags::default(), &env).unwrap();
            assert_eq!(config.host, "203.0.113.5");
            assert_eq!(config.port, 1234);
        }

        #[test]
        fn malformed_addr_env_is_ignored_not_a_hard_error() {
            let env = env_map(&[("CODEX_APP_SERVER_REST_ADDR", "not-an-address")]);
            let config = resolve_config(&CliFlags::default(), &env).unwrap();
            assert_eq!(config.host, "127.0.0.1");
            assert_eq!(config.port, 43210);
        }

        #[test]
        fn malformed_port_flag_is_a_hard_error() {
            let flags = CliFlags {
                port: Some("not-a-port".to_owned()),
                ..CliFlags::default()
            };
            let error = resolve_config(&flags, &no_env()).unwrap_err();
            assert!(error.contains("invalid --port"), "{error}");
        }

        #[test]
        fn out_of_range_port_flag_is_a_hard_error() {
            let flags = CliFlags {
                port: Some("70000".to_owned()),
                ..CliFlags::default()
            };
            let error = resolve_config(&flags, &no_env()).unwrap_err();
            assert!(error.contains("invalid --port"), "{error}");
        }

        #[test]
        fn unknown_mode_is_a_hard_error() {
            let flags = CliFlags {
                mode: Some("bogus".to_owned()),
                ..CliFlags::default()
            };
            let error = resolve_config(&flags, &no_env()).unwrap_err();
            assert!(error.contains("invalid --mode"), "{error}");
        }

        #[test]
        fn blank_token_is_treated_as_unset() {
            let flags = CliFlags {
                token: Some("   ".to_owned()),
                ..CliFlags::default()
            };
            let config = resolve_config(&flags, &no_env()).unwrap();
            assert_eq!(config.token, None);
        }
    }

    mod safety {
        use super::*;

        fn config(
            mode: Mode,
            host: &str,
            token: Option<&str>,
            allow_unsafe: bool,
        ) -> ResolvedConfig {
            ResolvedConfig {
                host: host.to_owned(),
                port: 43210,
                mode,
                token: token.map(str::to_owned),
                allow_unsafe_client_options: allow_unsafe,
            }
        }

        #[test]
        fn text_turn_mode_never_requires_a_token() {
            let outcome = check_safety(&config(Mode::TextTurn, "0.0.0.0", None, false)).unwrap();
            assert!(outcome.warnings.is_empty());
        }

        #[test]
        fn health_only_mode_never_requires_a_token() {
            let outcome = check_safety(&config(Mode::HealthOnly, "0.0.0.0", None, false)).unwrap();
            assert!(outcome.warnings.is_empty());
        }

        #[test]
        fn trusted_bridge_non_loopback_without_token_is_refused() {
            let error =
                check_safety(&config(Mode::TrustedBridge, "0.0.0.0", None, false)).unwrap_err();
            assert!(error.contains("refusing to bind"), "{error}");
            assert!(error.contains("--token"), "{error}");
        }

        #[test]
        fn trusted_bridge_non_loopback_with_token_is_allowed_with_no_warning_for_that_reason() {
            let outcome = check_safety(&config(
                Mode::TrustedBridge,
                "0.0.0.0",
                Some("secret"),
                false,
            ))
            .unwrap();
            assert!(outcome.warnings.is_empty());
        }

        #[test]
        fn trusted_bridge_loopback_without_token_warns_but_is_allowed() {
            let outcome =
                check_safety(&config(Mode::TrustedBridge, "127.0.0.1", None, false)).unwrap();
            assert_eq!(outcome.warnings.len(), 1);
            assert!(outcome.warnings[0].contains("no --token set"));
        }

        #[test]
        fn trusted_bridge_localhost_hostname_counts_as_loopback() {
            let outcome =
                check_safety(&config(Mode::TrustedBridge, "localhost", None, false)).unwrap();
            assert_eq!(outcome.warnings.len(), 1);
        }

        #[test]
        fn trusted_bridge_ipv6_loopback_counts_as_loopback() {
            let outcome = check_safety(&config(Mode::TrustedBridge, "::1", None, false)).unwrap();
            assert_eq!(outcome.warnings.len(), 1);
        }

        #[test]
        fn unsafe_client_options_non_loopback_without_token_is_refused() {
            let error = check_safety(&config(Mode::TextTurn, "0.0.0.0", None, true)).unwrap_err();
            assert!(
                error.contains("refusing --allow-unsafe-client-options"),
                "{error}"
            );
        }

        #[test]
        fn unsafe_client_options_with_token_warns_but_is_allowed() {
            let outcome =
                check_safety(&config(Mode::TextTurn, "0.0.0.0", Some("secret"), true)).unwrap();
            assert_eq!(outcome.warnings.len(), 1);
            assert!(outcome.warnings[0].contains("allow-unsafe-client-options is enabled"));
        }

        #[test]
        fn unsafe_client_options_on_loopback_warns_but_is_allowed() {
            let outcome = check_safety(&config(Mode::TextTurn, "127.0.0.1", None, true)).unwrap();
            assert_eq!(outcome.warnings.len(), 1);
        }

        #[test]
        fn both_refusals_can_combine_into_two_warnings_when_both_are_mitigated() {
            let outcome =
                check_safety(&config(Mode::TrustedBridge, "127.0.0.1", None, true)).unwrap();
            assert_eq!(outcome.warnings.len(), 2);
        }
    }

    mod loopback {
        use super::*;

        #[test]
        fn recognizes_loopback_forms() {
            assert!(is_loopback_host("127.0.0.1"));
            assert!(is_loopback_host("127.5.5.5"));
            assert!(is_loopback_host("::1"));
            assert!(is_loopback_host("localhost"));
            assert!(is_loopback_host("LOCALHOST"));
        }

        #[test]
        fn treats_everything_else_as_non_loopback() {
            assert!(!is_loopback_host("0.0.0.0"));
            assert!(!is_loopback_host("192.168.1.1"));
            assert!(!is_loopback_host("example.com"));
            assert!(!is_loopback_host("::"));
        }
    }

    /// End-to-end through [`run`] for the two argument-level exit paths that
    /// don't need a real socket: `--help` and `--version` both print and
    /// return before `resolve_config`/`bind_and_serve` run at all.
    mod run_entrypoints {
        use super::*;

        #[tokio::test]
        async fn help_flag_exits_success_without_binding_anything() {
            let code = run(strs(&["--help"]), &no_env()).await;
            assert_eq!(code, ExitCode::SUCCESS);
        }

        #[tokio::test]
        async fn version_flag_exits_success_without_binding_anything() {
            let code = run(strs(&["--version"]), &no_env()).await;
            assert_eq!(code, ExitCode::SUCCESS);
        }

        #[tokio::test]
        async fn bad_argument_exits_failure() {
            let code = run(strs(&["--bogus"]), &no_env()).await;
            assert_eq!(code, ExitCode::FAILURE);
        }

        #[tokio::test]
        async fn refused_safety_combination_exits_failure_without_binding() {
            let code = run(
                strs(&["--mode", "trusted-bridge", "--host", "0.0.0.0"]),
                &no_env(),
            )
            .await;
            assert_eq!(code, ExitCode::FAILURE);
        }
    }
}
