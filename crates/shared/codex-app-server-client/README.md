# codex-app-server-client

Standalone, fully-typed async Rust client for the [Codex CLI's `app-server`
v2 JSON-RPC protocol](https://developers.openai.com/codex/app-server) - the
interface Codex uses to power rich clients like the VS Code extension.

**This crate has zero path-dependencies on anything else in the workspace it
lives in.** Every dependency is a published crate from crates.io. It can be
copied into another project wholesale and will keep working.

- **MSRV:** Rust 1.97.1 | **Edition:** 2024 | **License:** MIT

## Contents

- [What it does](#what-it-does) | [What it deliberately doesn't do](#what-it-deliberately-doesnt-do)
- [Quick start](#quick-start) | [Cargo features](#cargo-features)
- [Batteries-included surface](#batteries-included-surface) — [`SessionOptions` builder](#sessionoptions-the-session-builder) | [Transports](#transports-stdio-unix-socket-arbitrary-streams) | [Approval handlers](#approval-handlers)
- [Optional REST adapter](#optional-rest-adapter) — [binary](#run-it-as-a-binary) | [mount](#mount-it-in-your-own-app) | [routes](#routes) | [bearer auth](#bearer-auth) | [CORS](#browser-clients-need-cors--the-adapter-adds-none) | [operational knobs](#operational-knobs) | [error model](#rest-error-model) | [custom backend](#custom-backend-pooling-tenancy-your-own-process-lifecycle) | [OpenAPI](#openapi) | [examples](#examples)
- [How the typed protocol layer is built](#how-the-typed-protocol-layer-is-built) | [Regenerating the schema](#regenerating-the-schema-after-upgrading-codex)
- [Reliability behavior](#reliability-behavior) | [Compatibility & versioning](#compatibility--versioning) | [License](#license)

## What it does

- Spawns `codex app-server` as a child process (or connects to an
  already-running one over a Unix socket, or any `AsyncRead + AsyncWrite`
  pair you hand it) and speaks its newline-delimited JSON-RPC 2.0 wire format.
- Exposes **every v2 client-request method** (122 in the `--experimental`
  surface, 87 without it - all compiled in; the app-server enforces the
  `experimentalApi` capability gate at runtime, not this crate) as a typed
  async function: `client.thread_start(params).await?`,
  `client.turn_start(params).await?`, `client.fs_read_file(params).await?`, ...
- Delivers the 68 server->client **notifications** (`turn/completed`,
  `item/agentMessage/delta`, ...) and the 11 server->client **requests**
  (command/file-change/permission approvals, tool-call user input, MCP
  elicitation, ...) through a single [`EventStream`], with typed reply
  helpers (`PendingServerRequest::respond` / `respond_error`) for the latter.
- Handles the `initialize` / `initialized` handshake, request-id correlation,
  and JSON-RPC error mapping.
- Provides batteries-included helpers on top of the generated protocol:
  `CodexSession` for one-call handshakes, builder constructors for common
  params, one-call text-turn helpers, `ApprovalHandler` policies for server
  requests, `EventCollector` for streamed turn output, `CodexDaemon` socket
  helpers, and `CompatibilityReport` for schema/version diagnostics.

## What it deliberately doesn't do

- No WebSocket transport - OpenAI's own docs mark it "experimental and
  unsupported." Stdio (the default) and Unix sockets cover the documented,
  supported surface.
- No native Codex app-server REST transport exists upstream. This crate's
  optional `rest` feature is an HTTP adapter that calls the real JSON-RPC
  app-server client underneath.
- No opinion on sandboxing or approval policy - that's all `codex` CLI/config
  territory, passed straight through via `extra_args` or the typed params
  structs. The `rest` feature ships an *optional* bearer-token layer
  (`rest::bearer_auth`) so every embedder isn't rewriting the same
  middleware, but it is transport auth only and off unless you mount it; the
  crate has no authorization or tenancy model.
- No retry/reconnect logic. One connection, one client. Build that on top if
  you need it.

## Quick start

Prerequisites: install the `codex` CLI, make sure `codex` is on `PATH`, and
complete any first-run login/setup before using turn-starting examples. The
`examples/basic.rs` connection smoke does not start a model turn, but this
quick start and other text-turn examples can consume model credits.

Downstream `Cargo.toml`:

```toml
[dependencies]
codex-app-server-client = { path = "crates/codex-app-server-client" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust,no_run
use codex_app_server_client::{CodexSession, DenyAllApprovalHandler, SessionOptions};

#[tokio::main]
async fn main() -> codex_app_server_client::Result<()> {
    let mut session = CodexSession::spawn(SessionOptions::new("my_integration", "0.1.0")).await?;
    let result = session
        .run_text_turn_with_model_and_handler(
            "gpt-5",
            "Say hello in one sentence.",
            &DenyAllApprovalHandler::default(),
        )
        .await?;

    println!("{}", result.agent_message());
    Ok(())
}
```

See:

- `examples/basic.rs` for a no-auth/no-turn smoke using `CodexSession`.
- `examples/session_turn.rs` for the one-call text-turn helper.
- `examples/approval_handler.rs` for routing server requests through a custom
  policy; use the documented preset handlers when they fit.
- `examples/daemon.rs` for Unix socket connection helpers.
- `examples/compatibility.rs` for schema/version diagnostics.
- `examples/rest_server.rs` for the optional HTTP bridge, and
  `examples/rest_loopback_dev.rs`, `examples/rest_bearer_auth.rs`,
  `examples/rest_trusted_gateway.rs`, `examples/rest_admin_unsafe.rs` for the
  four deployment postures (see "Optional REST adapter" below).
- `tests/smoke.rs` for a live integration test against the real binary
  (skips gracefully if `codex` isn't on `PATH`).

## Cargo features

| Feature | Default | Pulls in | Enables |
|---|---|---|---|
| *(none)* | ✓ | — | the full typed JSON-RPC client, `CodexSession`, approval handlers, `EventCollector`, `CodexDaemon`, `CompatibilityReport` |
| `rest` | | `axum`, `futures-core`, `tower-layer`, `tower-service`, and `tokio/rt-multi-thread` + `tokio/signal` | the [`rest`](#optional-rest-adapter) HTTP adapter, `rest::openapi_spec()`, and the `codex-app-server-rest` binary |

The default (feature-less) build is a pure library with no HTTP surface. The
`rest` feature is purely additive; the multi-threaded runtime and signal
handling it pulls in are used only by the binary, so a library-only consumer
never pays for them. No feature depends on anything outside crates.io.

## Batteries-included surface

The generated low-level methods are still available directly through
`CodexAppServerClient`, but most integrations should start with:

- `SessionOptions` + `CodexSession::spawn(...)`: spawn, initialize, send
  `initialized`, keep the client and event stream together, and expose helpers
  for starting threads, sending turns, and draining events.
- `CodexSession::run_text_turn(prompt)`: start a default thread, send one text
  turn, drain events until completion, and return a `TextTurnResult`. It uses
  `DenyAllApprovalHandler`, so it is best for read-only/smoke prompts.
- `CodexSession::run_text_turn_with_model_and_handler(model, prompt, handler)`:
  the same one-shot text flow with an explicit model and approval policy.
- `CodexSession::start_thread_with_model(model)`: start a thread without
  manually building `ThreadStartParams` when the only override is the model.
- `CodexSession::send_text_turn(thread_id, prompt)`: send a single text
  `UserInput` to an existing thread.
- `CodexSession::wait_for_turn_completed(thread_id, turn_id, handler)`: drain
  notifications for one turn and return the populated `EventCollector`.
- `CodexSession::collect_agent_message(thread_id, turn_id, handler)`: drain a
  turn and return only the concatenated assistant message text.
- `TextTurnResult`: bundles the `thread/start` response, `turn/start`
  response, and collected events; convenience accessors expose
  `agent_message()`, `latest_diff()`, and `errors()`.
- `ClientInfo::new`, `InitializeParams::for_client`,
  `ThreadStartParams::new`, `TurnStartParams::text`, `UserInput::text`,
  `ConfigReadParams::for_cwd`: common constructors that avoid ad hoc JSON for
  the first mile.
- `ApprovalHandler`, `ServerRequestReply`, and policy helpers:
  `DenyAllApprovalHandler` rejects every server request with a JSON-RPC error;
  `ReadOnlyApprovalHandler` answers `currentTime/read` and declines
  command/file-change prompts; `AllowAllApprovalHandler` approves command,
  file-change, legacy command/patch, and permission-profile approval requests;
  `FnApprovalHandler` lets you route each typed `ServerRequest` through custom
  sync logic; `AsyncFnApprovalHandler` and the `ApprovalFuture` alias let UI,
  channel, or service-backed policies await a decision without blocking a
  Tokio worker. Preset handlers intentionally return clear errors for dynamic
  tool calls, auth refreshes, and other app-specific requests they cannot
  answer safely.
- `EventCollector`: collect streamed agent text, latest diff, completion, and
  turn errors from `ServerNotification`s.
- `CodexDaemon`: build real `codex app-server --listen unix://...` args and
  connect to an existing Unix socket with the same session handshake.
- `CodexAppServerClient::call_raw_method(...)`: dynamic JSON-RPC method calls
  for bridge layers and generated surfaces that need to route methods by name.
- `CompatibilityReport::current()`: compare the installed `codex --version`
  with the vendored schema stamp and print method-count diagnostics.

### `SessionOptions`: the session builder

`SessionOptions::new(name, version)` is the minimum; everything else is an
optional chained builder:

| Method | Effect |
|---|---|
| `.with_title(title)` | sets the client's display title in `initialize` |
| `.with_command(command)` | runs a different executable than `codex` (e.g. an absolute path or wrapper) |
| `.with_extra_arg(arg)` | appends an argv entry to the spawned process (e.g. `--experimental`); call repeatedly |
| `.with_config(key, value)` | passes a `-c key=value` config override to `codex` |
| `.with_capabilities(caps)` | supplies explicit `InitializeCapabilities` instead of the defaults |
| `.with_call_timeout(duration)` | overrides the per-call request/response timeout (`DEFAULT_CALL_TIMEOUT`, 120s) |
| `.with_events_capacity(n)` | sizes the per-session event channel (`DEFAULT_EVENTS_CHANNEL_CAPACITY`, 1024); a slow consumer drops notifications past this, but never server *requests* — see [Reliability behavior](#reliability-behavior) |

```rust,no_run
use codex_app_server_client::{CodexSession, SessionOptions};
use std::time::Duration;

# async fn run() -> codex_app_server_client::Result<()> {
let options = SessionOptions::new("my_integration", "0.1.0")
    .with_extra_arg("--experimental")
    .with_call_timeout(Duration::from_secs(180))
    .with_events_capacity(4096);
let mut session = CodexSession::spawn(options).await?;
# let _ = &mut session;
# Ok(())
# }
```

### Transports: stdio, Unix socket, arbitrary streams

The client speaks the same NDJSON JSON-RPC 2.0 wire format over three
transports; pick whichever fits how you already run `codex app-server`:

- **Spawned child (stdio, the default):** `CodexSession::spawn(options)` or
  `CodexAppServerClient::spawn(command, extra_args)` fork `codex app-server`
  and own its lifecycle.
- **Existing Unix socket:** `CodexSession::connect_unix(path, options)` /
  `CodexAppServerClient::connect_unix(path)` attach to a running
  `codex app-server --listen unix://PATH` daemon (build those args with
  `CodexDaemon`).
- **Any `AsyncRead + AsyncWrite` pair:** `CodexSession::connect_streams(reader,
  writer, options)` / `CodexAppServerClient::connect_streams(reader, writer)`
  drive the protocol over a stream pair you already hold (a custom transport,
  a test harness, a tunnel).

Each `CodexAppServerClient` constructor has a `*_with_events_capacity` variant
(`spawn_with_events_capacity`, `connect_streams_with_events_capacity`,
`connect_unix_with_events_capacity`) for sizing the event channel directly when
you build the low-level client rather than going through `SessionOptions`.

There is deliberately no WebSocket transport — OpenAI marks it experimental and
unsupported.

### Approval handlers

Server-originated requests (command/file-change/permission approvals,
elicitation, tool-call input) are answered by an `ApprovalHandler`. Pick a
preset or supply your own:

| Handler | Behavior |
|---|---|
| `DenyAllApprovalHandler` | rejects every server request with a JSON-RPC error — safe for read-only/smoke prompts |
| `ReadOnlyApprovalHandler` | answers `currentTime/read`, declines command/file-change prompts |
| `AllowAllApprovalHandler` | approves command, file-change, legacy command/patch, and permission-profile requests |
| `FnApprovalHandler` | routes each typed `ServerRequest` through your sync closure |
| `AsyncFnApprovalHandler` | the same, but your closure returns an `ApprovalFuture` so UI/channel/service-backed policies can await a decision without blocking a Tokio worker |

Presets intentionally return clear errors for requests they can't answer
safely (dynamic tool calls, auth refreshes, other app-specific flows) rather
than guessing. Reply plumbing for the raw path is `PendingServerRequest`'s
`respond` / `respond_error` (see `ServerRequestReply`).

## Optional REST adapter

Enable the `rest` feature when you want a portable HTTP bridge for the Codex
app-server protocol:

```toml
[dependencies]
codex-app-server-client = { path = "crates/codex-app-server-client", features = ["rest"] }
axum = "0.8"
tokio = { version = "1", features = ["macros", "net", "rt-multi-thread"] }
```

The adapter is deliberately self-contained: it uses crates.io dependencies
only, has no Soma path-dependencies, and does not assume any auth, gateway,
Labby, Beads, or repo-specific runtime.

The REST adapter is only an adapter around local `codex app-server` processes.
It does not authorize requests, sandbox clients, or make the upstream
app-server safe to expose on a network. It ships an *optional* bearer-token
layer (see "Bearer auth" below), but that is transport auth only: it answers
"did the caller present the secret?", not "may this caller do this?". Bind it
to loopback or place it behind your own authentication and authorization
layer.

### Run it as a binary

The fastest path — no Rust code at all. From this crate's directory:

```bash
cargo install --path . --features rest
codex-app-server-rest --host 127.0.0.1 --port 43210 --mode text-turn
```

The flags (each with a `CODEX_APP_SERVER_REST_*` env fallback; the flag wins
when both are set):

| Flag | Env fallback | Default | Meaning |
|---|---|---|---|
| `--host <HOST>` | `…_HOST` (or `…_ADDR` for `host:port`) | `127.0.0.1` | bind host |
| `--port <PORT>` | `…_PORT` (or `…_ADDR`) | `43210` | bind port |
| `--mode <MODE>` | `…_MODE` | `text-turn` | router: `health-only`, `text-turn`, or `trusted-bridge` |
| `--token <TOKEN>` | `…_TOKEN` | — | require `Authorization: Bearer <TOKEN>` (bearer auth on) |
| `--allow-unsafe-client-options` | — | off | admin-only: let callers override `command`/`extraArgs`/`config`/`approvalPolicy` |
| `--help`, `--version` | — | — | print usage / version and exit |

`--mode` selects the router: `health-only` (health + compatibility only),
`text-turn` (adds the one-shot text helper), or `trusted-bridge` (adds the raw
session/call/event routes). Resource limits come from the
[`CODEX_APP_SERVER_REST_*` env knobs](#operational-knobs). The binary prints
its effective configuration (mode, bind, auth on/off, unsafe options, and all
resolved limits — never the token) on startup.

The binary refuses to start rather than let you deploy something dangerous by
accident: a non-loopback bind in `--mode trusted-bridge` without `--token` is
rejected, as is `--allow-unsafe-client-options` on a non-loopback bind without
a token.

On `SIGTERM` (`systemd` stop, `docker stop`, an orchestrator rolling the pod)
or `ctrl-c`, it shuts down gracefully: it stops accepting new connections and
drains in-flight requests before exiting, rather than dropping active sessions
and orphaning their `codex app-server` children.

### Mount it in your own app

```rust,no_run
use codex_app_server_client::rest;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let listener = tokio::net::TcpListener::bind("127.0.0.1:43210").await?;
axum::serve(listener, rest::text_turn_router()).await?;
# Ok(())
# }
```

### Routes

Always mounted (every router constructor):

- `GET /health` and `GET /v1/health`: liveness probe.
- `GET /v1/compatibility`: schema stamp, installed `codex --version`, and
  generated method-count summary.
- `POST /v1/text-turn`: mounted by `rest::text_turn_router()` or
  `RestRouterOptions::text_turn()`. It starts a fresh Codex session, sends one
  text prompt, waits for turn completion, and returns assistant text, latest
  diff, and turn errors.

`rest::router()` mounts only the non-executing health and compatibility routes.
`rest::text_turn_router()` opts into the one-shot text helper. Unsafe client
options are off by default: the REST layer rejects `approvalPolicy:
"allow_all"` plus client `command`, `extraArgs`, and `config` overrides because
those controls can change what the local Codex process is allowed to run.

For a full stateful REST bridge to every callable, mount the trusted router
only behind your own authentication and authorization boundary:

```rust,no_run
use codex_app_server_client::rest;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let listener = tokio::net::TcpListener::bind("127.0.0.1:43210").await?;
axum::serve(listener, rest::trusted_bridge_router()).await?;
# Ok(())
# }
```

**Trusted bridge warning:** `trusted_bridge_router()` has no built-in auth and
must never be exposed publicly. It enables the raw callable/session routes, but
unsafe client options (`command`, `extraArgs`, `config`, and
`approvalPolicy: "allow_all"`) still remain disabled. Enable those only for an
operator-owned/admin-only boundary with
`RestRouterOptions::trusted_bridge().with_unsafe_client_options(true)`.

Trusted bridge routes:

- `POST /v1/call/{method}`: one-shot raw JSON-RPC bridge. The backend starts
  a fresh session, calls the app-server method named by the path (for example
  `/v1/call/config/read` or `/v1/call/thread/start`), and returns the raw
  result. This is useful for single request/response calls, but it does not
  preserve turn state, stream events, or let you answer server-originated
  requests after the call returns.
- `POST /v1/sessions`: starts a persistent app-server session and returns a
  `sessionId` plus the raw initialize response.
- `GET /v1/sessions`: lists active REST bridge sessions.
- `DELETE /v1/sessions/{sessionId}`: drops a bridge session and terminates the
  owned app-server process when no client clones remain.
- `POST /v1/sessions/{sessionId}/call/{method}`: calls any app-server method
  on an existing session, preserving thread/event state across calls.
- `GET /v1/sessions/{sessionId}/events?timeoutMs=30000`: long-polls the next
  server notification/request. Server requests are returned with a
  `requestKey` so REST clients can answer them later.
- `GET /v1/sessions/{sessionId}/events/stream?timeoutMs=30000`: the same
  events as Server-Sent Events (`text/event-stream`) instead of one
  long-poll per event — usually what you want from a browser or any client
  with an `EventSource`. Each event arrives as one `data:` frame carrying the
  identical JSON payload the long-poll route returns, tagged with an SSE
  `event:` name (`notification`, `request`, `closed`, `timeout`, `error`).
  The stream ends on `closed` or `error`; a `timeout` is forwarded and the
  stream continues.
- `POST /v1/sessions/{sessionId}/requests/{requestKey}/result`: replies to a
  pending server-originated request with a JSON-RPC `result`.
- `POST /v1/sessions/{sessionId}/requests/{requestKey}/error`: replies to a
  pending server-originated request with a JSON-RPC `error`.

Use the session routes, event polling, and request reply routes for the "every
callable" bridge contract. Codex turns are stateful: start a bridge session,
call `thread/start`, call `turn/start` with the returned thread id, read
`/events` (long-poll) or `/events/stream` (SSE), and answer any returned
`request` events before waiting for `turn/completed`.

A session has **at most one active event consumer**. `/events` and
`/events/stream` drain the same underlying stream, so a second concurrent
reader for the same session — of either kind — gets `409 Conflict` rather
than silently splitting events between two readers. Pick one per session.

One-shot text helper request. `approvalPolicy` is one of `deny_all` (default),
`read_only`, or `allow_all` — and `allow_all` is rejected with `403` unless the
router was built with `with_unsafe_client_options(true)`. The `client` object
is optional; its `command`/`extraArgs`/`config` fields are the "unsafe" options
gated the same way.

```json
{
  "prompt": "Say hello in one sentence.",
  "model": "gpt-5",
  "approvalPolicy": "read_only",
  "client": {
    "name": "my_rest_client",
    "version": "0.1.0",
    "callTimeoutMs": 120000
  }
}
```

One-shot text helper response:

```json
{
  "threadId": "019...",
  "turnId": "019...",
  "turnStatus": "completed",
  "agentMessage": "Hello.",
  "latestDiff": null,
  "errors": []
}
```

One-shot raw bridge request:

```http
POST /v1/call/thread/start
content-type: application/json
```

```json
{
  "params": {
    "model": "gpt-5",
    "cwd": "/workspace"
  },
  "client": {
    "name": "my_rest_client",
    "version": "0.1.0",
    "extraArgs": ["--experimental"]
  }
}
```

One-shot raw bridge response:

```json
{
  "method": "thread/start",
  "result": {
    "thread": {
      "id": "019..."
    }
  }
}
```

Stateful bridge flow:

```http
POST /v1/sessions
content-type: application/json
```

```json
{
  "client": {
    "name": "my_rest_client",
    "version": "0.1.0"
  }
}
```

```json
{
  "sessionId": "session-1",
  "initializeResponse": {
    "platformOs": "linux"
  }
}
```

```http
POST /v1/sessions/session-1/call/thread/start
content-type: application/json
```

```json
{
  "params": {
    "model": "gpt-5",
    "cwd": "/workspace"
  }
}
```

```json
{
  "method": "thread/start",
  "result": {
    "thread": {
      "id": "019..."
    }
  }
}
```

```http
POST /v1/sessions/session-1/call/turn/start
content-type: application/json
```

```json
{
  "params": {
    "threadId": "019...",
    "input": [{ "type": "text", "text": "Hello" }]
  }
}
```

```http
GET /v1/sessions/session-1/events?timeoutMs=30000
```

Event polling response shapes:

```json
{ "event": "timeout" }
```

```json
{
  "event": "notification",
  "notification": {
    "method": "turn/completed",
    "params": {}
  }
}
```

```json
{
  "event": "request",
  "requestKey": "request-1",
  "requestId": 42,
  "method": "currentTime/read",
  "request": {
    "id": 42,
    "method": "currentTime/read",
    "params": {}
  }
}
```

Request reply examples:

```json
{ "result": { "currentTimeAt": 1760000000 } }
```

```json
{
  "code": -32000,
  "message": "denied",
  "data": null
}
```

### Bearer auth

Auth stays optional, but you shouldn't have to rewrite the same middleware to
get it:

```rust,no_run
use codex_app_server_client::rest;

# fn build(token: String) -> axum::Router {
rest::trusted_bridge_router().layer(rest::bearer_auth(token))
# }
```

Every request then needs `Authorization: Bearer <token>`. The comparison is
constant-time, a blank configured token panics at construction rather than
silently accepting blank credentials, and rejections return the adapter's
normal `RestErrorResponse` JSON with `WWW-Authenticate: Bearer`.

`GET /health` and `GET /v1/health` are exempt by default — liveness probes
rarely carry credentials and "the process is up" leaks nothing. Flip that with
`rest::bearer_auth(token).allow_unauthenticated_health(false)`.
`GET /v1/compatibility` is **never** exempt, because it reveals the installed
`codex` version.

This is transport auth only. A caller holding the one shared token gets
everything the mounted router exposes; it is not multi-tenant isolation and it
is not authorization.

### Browser clients need CORS — the adapter adds none

The SSE route exists for browser clients, but **this crate sets no CORS
headers**, deliberately: a `CorsLayer` would mean a `tower-http` dependency,
and the crate is kept to a minimal, audited dependency graph so it stays
liftable (see the top of this file). A page served from a different origin
than the adapter therefore can't call any of these routes — including
`EventSource` against `/events/stream` — until *you* add CORS in front of it.

Add it in the host application (it's one layer):

```rust,ignore
use tower_http::cors::CorsLayer;

let app = codex_app_server_client::rest::trusted_bridge_router()
    .layer(CorsLayer::permissive()); // scope this to your real origins in production
```

`CorsLayer::permissive()` is fine for local development; in production restrict
it to the specific origins that should reach the adapter. Same-origin
deployments (the page and the adapter behind one gateway/host) need nothing
here.

### Operational knobs

Every limit has a default and a `CODEX_APP_SERVER_REST_*` environment
override. Build them with `RestLimits::from_env()` (panics on a malformed
value) or `RestLimits::try_from_env()` (returns `RestLimitsEnvError`), then
`RestRouterOptions::trusted_bridge().with_limits(limits)`.

| Env var (`CODEX_APP_SERVER_REST_` +) | Default | Bounds |
|---|---|---|
| `MAX_SESSIONS` | `16` | concurrent stateful bridge sessions |
| `MAX_ONE_SHOT_CONCURRENCY` | `4` | in-flight `/v1/text-turn` + `/v1/call/*` |
| `MAX_SESSION_CALL_CONCURRENCY` | `64` | in-flight session calls, all sessions |
| `MAX_SESSION_CALL_CONCURRENCY_PER_SESSION` | `8` | in-flight session calls, one session |
| `MAX_POLL_TIMEOUT_MS` | `30000` | ceiling on `?timeoutMs=` for both event routes |
| `MIN_STREAM_POLL_TIMEOUT_MS` | `250` | floor on `?timeoutMs=`, SSE route only |
| `MAX_TEXT_TURN_DURATION_MS` | `600000` | wall-clock budget for one text turn |
| `MAX_TEXT_TURN_OUTPUT_BYTES` | `1048576` | response byte cap for one text turn |
| `MAX_REQUEST_BODY_BYTES` | `2097152` | request body cap, all routes (`413` past it) |
| `PENDING_REQUEST_TTL_MS` | `600000` | how long an unanswered server request lives |
| `MAX_PENDING_REQUESTS_PER_SESSION` | `64` | unanswered server requests per session |
| `EVENTS_CHANNEL_CAPACITY` | `1024` | event buffer per session; events drop once full |
| `IDLE_SESSION_TTL_MS` | `1800000` | idle session reaping |
| `COMPATIBILITY_TTL_MS` | `30000` | `/v1/compatibility` cache |
| `SSE_KEEP_ALIVE_MS` | `15000` | SSE keep-alive frame interval |

Only the SSE route has a floor on `?timeoutMs=`, and the asymmetry is
deliberate. On the long-poll route `timeoutMs=0` means "tell me only if an
event is already waiting" — a useful non-blocking poll, and each repeat costs
the caller an HTTP round trip that paces it. A stream has neither: it is one
request that loops server-side for as long as the client reads, so a zero
timeout there turns a single request into an unbounded run of back-to-back
backend polls. The floor costs real events nothing — `poll_event` returns as
soon as an event arrives, so the timeout only bounds the *idle* wait — it just
caps how often an idle stream reports that nothing happened.

A variable that is present but unparseable is a hard error, never a silent
fallback to the default — that is how a 10x-wrong limit ships unnoticed.

### REST error model

Every error response is the same JSON envelope, `RestErrorResponse`:

```json
{ "error": "not_found", "message": "session `x` was not found", "code": null, "data": null }
```

`error` is a stable machine-readable kind; `message` is human-readable; `code`
and `data` are populated only when the failure is a JSON-RPC error forwarded
from the underlying `codex app-server` (they carry its numeric code and data).
The status codes the adapter emits:

| Status | `error` kind | When |
|---|---|---|
| `400` | `invalid_request` / `invalid_json` | malformed body or bad request shape |
| `401` | `unauthorized` | bearer auth mounted and the token is missing/wrong |
| `403` | `forbidden` | an unsafe client option was requested without opting in |
| `404` | `not_found` | unknown session, method, or pending request |
| `409` | `conflict` | a second event consumer for a session that already has one |
| `410` | `gone` | a server request expired before it could be answered |
| `413` | `payload_too_large` | request body over `MAX_REQUEST_BODY_BYTES`, or turn output over `MAX_TEXT_TURN_OUTPUT_BYTES` |
| `429` | `rate_limited` | a concurrency/session limit is saturated |
| `500` | `internal` | an adapter-internal failure (e.g. a check task panicked) |
| `502` | `json_rpc_error` / `codex_app_server_error` | the app-server process failed, disconnected, or returned a JSON-RPC error |
| `504` | `timeout` | a text turn exceeded `MAX_TEXT_TURN_DURATION_MS` |

On the SSE route, a failure that would be `404`/`409`/`410`/`429` on the
long-poll route instead ends the stream with a terminal `event: error` frame
carrying the same envelope (minus the HTTP status, since headers are already
committed) — see the [routes](#routes) section.

### Custom backend (pooling, tenancy, your own process lifecycle)

The routers default to `CodexRestBackend`, which spawns and pools real
`codex app-server` children. To control process lifecycle, pooling, tenancy,
or policy yourself, implement the `RestBackend` trait and mount it:

```rust,ignore
use codex_app_server_client::rest::{self, RestRouterOptions};

let router = rest::router_with_backend_and_options(
    my_backend, // impl RestBackend
    RestRouterOptions::trusted_bridge(),
);
```

`RestBackend` has one required method per capability (`compatibility_report`,
`run_text_turn`, and the session/call/event/reply methods), each with a
default that returns a `not_found` error — so a backend only implements the
routes it actually serves. `router_with_backend(...)` and
`router_with_backend_arc(...)` are shorthands for the default (non-executing)
options.

### OpenAPI

`rest::openapi_spec()` returns an OpenAPI 3.1.0 document for the whole `rest`
surface, and the same document is checked in at
[`openapi.json`](openapi.json) so downstream clients can be generated without
building the Rust crate at all. A test keeps the two in sync; regenerate with:

```bash
CODEX_REST_OPENAPI_WRITE=1 cargo test -p codex-app-server-client \
  --features rest openapi_spec_matches_checked_in_file
```

A generated TypeScript client lives in
[`clients/typescript/`](clients/typescript) — proof the surface is consumable
outside Rust, and a starting point for any other language.

### Examples

```bash
cargo run -p codex-app-server-client --features rest --example rest_server
```

Set `CODEX_APP_SERVER_REST_ADDR=127.0.0.1:43211` to pick a different bind
address. Four further examples walk the deployment postures in increasing
order of danger — read the one matching your deployment before copying it:

| Example | Posture |
|---|---|
| `rest_loopback_dev` | no auth, loopback only; the "just let me try it" path |
| `rest_bearer_auth` | token-protected local service |
| `rest_trusted_gateway` | full bridge behind someone else's authz boundary |
| `rest_admin_unsafe` | admin-only `allow_unsafe_client_options` |

Host applications that want pooling, auth, tenancy, or their own process
lifecycle can mount `rest::router_with_backend(...)` for non-executing
health/compat routes, `rest::router_with_backend_and_options(backend,
rest::RestRouterOptions::text_turn())` for the text helper, or
`rest::router_with_backend_and_options(backend,
rest::RestRouterOptions::trusted_bridge())` for the full trusted bridge.

## How the typed protocol layer is built

`build.rs` runs two codegen passes against assets vendored in `schema/`:

1. **`schema/protocol.schema.json`** (660 JSON Schema definitions) is fed
   through [`typify`](https://github.com/oxidecomputer/typify) to generate
   every request/response/notification/payload type in [`protocol`]. This is
   the *v2-only* subset of Codex's own generated schema, merged with the few
   JSON-RPC envelope and `ServerRequest`/`ClientNotification` types that
   Codex's generator doesn't currently version-split. See
   `xtask/src/codex_schema/merge.rs`'s module docs for exactly how that merge
   works and why (short version: Codex's `codex app-server
   generate-json-schema` emits a legacy-plus-v2 mixed bundle; this crate only
   wants v2).
2. **`schema/methods.json`** (method name -> params/response type names,
   also derived by `xtask/src/codex_schema/merge.rs`) drives generation of
   one ergonomic wrapper method per client-request method, plus the
   `PendingServerRequest` reply plumbing for the 11 server->client requests.

A known `typify` 0.7.0 limitation is worked around in
`xtask/src/codex_schema/merge.rs`: `McpServerElicitationRequestParams`
combines a top-level object with a sibling `oneOf` where one branch contains
a wildcard (`true`) sub-schema, which panics typify's schema-merge logic
(`typify-impl-0.7.0/src/merge.rs:427`, "not yet implemented"). The merge step
flattens the shared base fields into each `oneOf` branch first, producing an
equivalent, typify-friendly schema with no loss of type fidelity (verified:
the flattened version generates a correctly `#[serde(tag = "mode")]`
discriminated enum with each variant's own concrete field types).

## Regenerating the schema (after upgrading `codex`)

The schema used to be regenerated by a standalone `schema/build_combined_schema.py`
script - this repo is otherwise all-Rust, so that logic has been ported to a
`cargo xtask` subcommand instead (`xtask/src/codex_schema/`); the Python
script is gone.

```bash
# 1. Dump codex's own protocol bundles at the new version:
codex app-server generate-json-schema --out /tmp/codex-schema --experimental

# 2. Rebuild schema/protocol.schema.json + schema/methods.json from that dump,
#    and stamp schema/CODEX_VERSION.txt with the codex version used:
cargo xtask codex-schema regen /tmp/codex-schema

# 3. Rebuild and re-verify:
cargo build -p codex-app-server-client --all-targets
cargo clippy -p codex-app-server-client --all-targets -- -D warnings
cargo test -p codex-app-server-client
```

**Staleness warnings.** `build.rs` does a best-effort check on every build: if
a `codex` binary is on `PATH`, it compares `codex --version` against the
version stamped in `schema/CODEX_VERSION.txt` by the last `regen` run. A
mismatch emits a non-fatal `cargo:warning` pointing back at this section -
never a build failure, and this crate still builds fine on machines/CI
without `codex` installed at all (the check is skipped silently in that
case).

If `typify` starts panicking on a *different* type after a schema change,
that means the new version introduced another schema shape it can't merge.
Bisect it automatically instead of doing the "opaque out half the
new/changed definitions, see if the panic goes away" search by hand:

```bash
cargo xtask codex-schema bisect /tmp/codex-schema
```

This binary-searches the definitions that are new or changed versus the
currently committed `schema/protocol.schema.json` (falling back to searching
every definition if there's no usable baseline), opaquing out half the
remaining candidates at a time and re-probing typify, until it isolates the
minimal definition(s) that reproduce the panic - the same process used by
hand to find the `McpServerElicitationRequestParams` case above. It reports
the culprit definition name(s) plus their raw JSON schema so you can decide
how to handle it: flatten the offending shape the same way (see
`xtask/src/codex_schema/merge.rs::flatten_base_plus_oneof`), or, as a last
resort, replace its definition with `true` (opaque `serde_json::Value`) and
document the loss of type fidelity.

## Reliability behavior

These were shaken out by an adversarial multi-agent review after the initial
implementation (see the session that added this crate) and are worth knowing
about even though they're not "limitations" per se:

- **Call timeout.** Every request/response round trip is bounded by
  `codex_app_server_client::DEFAULT_CALL_TIMEOUT` (120s), overridable per
  client via `CodexAppServerClient::with_call_timeout`. This only bounds the
  request/response itself, not how long a turn takes to finish generating -
  that streams via `Event::Notification`, not by blocking the request that
  started it.
- **Write-stall detection.** A single outgoing write is bounded by an internal
  30s timeout. If a peer stops draining its input (backpressure with nobody
  reading, not necessarily a crash), the writer task tears the connection down
  - explicitly shutting down the write half, killing a spawned child - rather
  than hanging forever with an ever-growing outgoing queue.
  Cancelling an in-flight call (e.g. wrapping a generated method in
  `tokio::time::timeout`) never leaks its entry in the pending-request map.
- **Bounded line size.** Incoming NDJSON lines are capped at
  `codex_app_server_client::MAX_LINE_BYTES` (64 MiB) to prevent unbounded memory growth from
  a single huge or unterminated line.
- **Undecodable server->client requests get an error reply, not silence.** If
  an incoming request (has both `method` and `id`) doesn't decode into the
  generated `ServerRequest` enum - e.g. a method added in a newer app-server
  version than this crate's vendored schema - the crate sends back a JSON-RPC
  error response instead of silently dropping it, so the app-server doesn't
  wait forever for a reply it'll never get. Undecodable notifications (no
  `id`, so no reply is expected either way) are logged and dropped.
- **Dropping every `CodexAppServerClient` clone reliably tears the connection
  down**, including killing a spawned child, independent of the reader task's
  own internal channel clones (which necessarily outlive individual calls).
  Shutdown is coordinated by a shared `tokio_util::sync::CancellationToken`,
  not just channel-closing, so it isn't defeated by any task that necessarily
  holds a longer-lived channel clone than the caller's own handles.
- **The writer task (and a spawned child) is reaped promptly on its own**,
  not just when every client clone is dropped: the reader task proactively
  cancels the shared token the moment it detects the connection is dead (EOF
  or a read error), so a crashed/exited app-server process gets cleaned up
  immediately - the caller doesn't need to notice `Event::Closed` and drop the
  client themselves for that to happen. The reader task itself also races
  against that same cancellation signal (not just EOF/errors), so dropping
  every client clone terminates it promptly even for `connect_streams`/
  `connect_unix` peers that never notice or react to the writer's half-close -
  and its cleanup (clearing pending calls, emitting `Event::Closed`,
  cancelling the writer) runs via a `Drop` guard, so it still happens even if
  a panic unwinds through the reader loop instead of exiting normally.
- **Bounded event channel.** `EventStream`'s internal channel holds up to
  1024 events (`EVENTS_CHANNEL_CAPACITY`); if a consumer falls behind,
  `Event::Notification`s are dropped (logged), but `Event::Request`s are
  never silently dropped - this crate replies with a fallback JSON-RPC error
  on the app-server's behalf instead of leaving it hanging.
- **No `PendingServerRequest` can leak indefinitely, and none is ever left
  permanently unanswered.** Dropping one - deliberately, via cancellation, or
  via a panic unwinding through it - sends a fallback JSON-RPC error reply
  through its own `Drop` impl, so the app-server always gets *some* reply,
  not just a resolved-with-nothing internal channel. An internal
  `PENDING_SERVER_REQUEST_TIMEOUT` (600s - generous, since these are often
  human-in-the-loop approval/elicitation flows) is a separate backstop for
  the different, rarer case of a caller holding one forever without ever
  dropping *or* responding to it (e.g. stored in a collection and forgotten).
  Always respond promptly and explicitly anyway - the fallbacks exist so a
  bug or an unhandled case doesn't turn into a silent hang, not as a
  substitute for handling every event you receive.
- **`RequestId` is a full `Eq + Hash` key type**, so it can be used directly
  as a `HashMap`/`HashSet` key with no caller-side newtype wrapper. This
  matters for anything built on top of this crate that needs to track
  server->client requests by id - e.g. a UI layer correlating in-flight
  approval/elicitation requests by their app-server-assigned `RequestId`, or
  a higher-level wrapper that pools/multiplexes several
  `CodexAppServerClient` connections and keys state by the id namespace of
  whichever connection a message came from.

## Compatibility & versioning

- **Codex protocol.** The vendored schema targets a specific `codex` version,
  stamped in `schema/CODEX_VERSION.txt` and reported by `CODEX_SCHEMA_VERSION`
  / `CompatibilityReport`. The surface counts are exposed as constants
  (`CLIENT_REQUEST_METHOD_COUNT`, `SERVER_REQUEST_METHOD_COUNT`,
  `SERVER_NOTIFICATION_METHOD_COUNT`, `CLIENT_NOTIFICATION_METHOD_COUNT`) and
  as `SurfaceSummary::current()`. When you upgrade `codex`, see
  [Regenerating the schema](#regenerating-the-schema-after-upgrading-codex);
  `cargo xtask codex-schema drift` reports whether the installed CLI has moved
  ahead of the vendored schema.
- **Rust.** MSRV is 1.97.1 (edition 2024). Raising it is a breaking change for
  consumers pinned to an older toolchain.
- **Crate API.** Pre-1.0 (`0.1.x`); the public API may change between minor
  versions. The `rest` wire surface is captured in [`openapi.json`](openapi.json),
  which is the contract downstream (non-Rust) clients should track.

## License

MIT.
