# codex-app-server-client

Standalone, fully-typed async Rust client for the [Codex CLI's `app-server`
v2 JSON-RPC protocol](https://developers.openai.com/codex/app-server) - the
interface Codex uses to power rich clients like the VS Code extension.

**This crate has zero path-dependencies on anything else in the workspace it
lives in.** Every dependency is a published crate from crates.io. It can be
copied into another project wholesale and will keep working.

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

## What it deliberately doesn't do

- No WebSocket transport - OpenAI's own docs mark it "experimental and
  unsupported." Stdio (the default) and Unix sockets cover the documented,
  supported surface.
- No opinion on auth, sandboxing, or approval policy - that's all `codex`
  CLI/config territory, passed straight through via `extra_args` or the
  typed params structs.
- No retry/reconnect logic. One connection, one client. Build that on top if
  you need it.

## Quick start

```rust,no_run
use codex_app_server_client::protocol::{ClientInfo, InitializeParams};
use codex_app_server_client::{CodexAppServerClient, Event};

#[tokio::main]
async fn main() -> codex_app_server_client::Result<()> {
    let (client, mut events) = CodexAppServerClient::spawn("codex", &[])?;

    client
        .initialize(InitializeParams {
            client_info: ClientInfo {
                name: "my_integration".into(),
                title: None,
                version: "0.1.0".into(),
            },
            capabilities: None,
        })
        .await?;
    client.send_initialized()?;

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                Event::Notification(n) => println!("{n:?}"),
                Event::Request(req) => req.respond_error(-1, "not handled", None),
                Event::Closed => break,
            }
        }
    });

    let thread = client
        .thread_start(serde_json::from_value(serde_json::json!({ "model": "gpt-5.4" }))?)
        .await?;
    println!("started thread {}", thread.thread.id);
    Ok(())
}
```

See `examples/basic.rs` for a runnable version that only calls
no-auth-required methods, and `tests/smoke.rs` for a live integration test
against the real binary (skips gracefully if `codex` isn't on `PATH`).

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
