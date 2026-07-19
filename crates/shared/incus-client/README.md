# incus-client

Standalone async Rust client for the [Incus REST API](https://linuxcontainers.org/incus/docs/main/rest-api/)
(system container and virtual machine management) over a **local Unix domain
socket**.

**This crate has zero path-dependencies on anything else in the workspace it
lives in.** Every dependency is a published crate from crates.io. It can be
copied into another project wholesale and will keep working.

**Unix-only.** Incus itself only runs on Linux, and this crate's transport is
a Unix domain socket with no cross-platform equivalent. The entire crate is
gated behind `#![cfg(unix)]` (`src/lib.rs`) and compiles to an empty, still
successfully-building crate on non-Unix targets (Windows CI, for instance)
rather than hard-failing a workspace build.

## What it does

- Hand-rolled HTTP/1.1-over-`UnixStream` transport (`src/transport/unix.rs`):
  a fresh connection per request (so concurrent calls never block each
  other), sync/async/error envelope parsing, and defensive caps everywhere a
  peer controls a size (response body, cumulative response headers, chunked
  transfer-encoding chunks).
- Async-operation lifecycle (`src/operations.rs`): every mutation Incus
  documents as long-running returns an `Operation`; `wait_for_operation`
  waits for it to finish via Incus's long-poll `.../wait` endpoint.
- Optional WebSocket subscription to Incus's `/1.0/events` push stream
  (`src/events.rs`, behind the `events` feature) - an enhancement over
  `wait_for_operation`, not a replacement for it.
- Full CRUD + lifecycle + snapshots for **instances** (containers and VMs);
  CRUD for **images**, **networks**, **projects**, and **storage
  pools/volumes** (`src/resources/`).
- ETag/`If-Match` optimistic-concurrency support on instances, images,
  networks, and projects (not yet on storage pools/volumes - see [Known
  limitations](#known-limitations--follow-up-work)).
- A single, structured `Error` enum (`src/error.rs`) - every failure mode a
  caller might need to branch on (a stale ETag, a request timeout, a
  non-cancellable operation, ...) is a named variant, not a string to parse.

## What it deliberately doesn't do

- **No remote/TLS transport.** Incus also supports a remote mutual-TLS HTTPS
  transport with a trust-on-first-use certificate model, but that surface is
  *not* implemented here - a from-scratch TLS trust implementation carries
  real security risk that isn't worth taking on speculatively. Tracked
  separately as beads epic `rmcp-template-21b7`, pending a real remote
  consumer. Until then, this crate only ever talks to a local Incus daemon.
- **No certificates CRUD.** Only has a real consumer once the mTLS transport
  above exists.
- **No `?filter=` (OData-style) query support.** Every `list_*` method
  supports Incus's `recursion` parameter; server-side filtering is out of
  scope for v1.
- **No instance exec, console attach, or file push/pull.** Each of those
  uses `POST .../exec`-style operations whose `metadata` carries secrets for
  separate control/stdin/stdout WebSocket connections - a materially
  different protocol from the generic operations/events model the rest of
  this crate is built on.
- **No global "list every volume across every pool."** Incus has no such
  endpoint; volumes are inherently scoped under a pool
  (`/1.0/storage-pools/{pool}/volumes`). Fan out over `list_storage_pools`
  yourself if you need this.
- **No retry/reconnect logic.** One connection per request, no session
  state to reconnect. Build retry policy on top if you need it (see the
  [Error](#errors) reference for what's safe to retry).

## Quick start

```toml
[dependencies]
incus-client = { path = "crates/shared/incus-client" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust,no_run
use incus_client::{Client, ClientConfig};

#[tokio::main]
async fn main() -> incus_client::Result<()> {
    let client = Client::new(ClientConfig::unix_socket("/var/lib/incus/unix.socket"));

    // recursion = false: cheap, name/URL references only.
    let names = client.list_instances(false).await?;
    println!("{names:?}");

    // recursion = true: every instance's full object (config/devices/state).
    let full = client.list_instances(true).await?;
    println!("{full:?}");

    // Start an instance and wait for the operation to finish. `None` waits
    // indefinitely, transparently re-polling Incus's server-side long-poll.
    let op = client.start_instance("web-01").await?;
    client.wait_for_operation(op.id, None).await?;

    Ok(())
}
```

This snippet is kept honest by `examples/basic.rs` - a real, compiled
(via `cargo build -p incus-client --example basic`, part of the normal CI
build) copy of it. It needs a live Incus daemon to actually *run*, so it
isn't executed in CI, but the compiler still catches any drift between this
README and the crate's real API:

```bash
cargo run -p incus-client --example basic -- /var/lib/incus/unix.socket
```

## Feature flags

| Feature | Default | Adds |
|---|---|---|
| *(none)* | - | Unix-socket transport, operations, all five resource modules. |
| `events` | off | `dep:futures`, `dep:tokio-tungstenite` (no TLS feature - the events WebSocket runs over the same plain Unix socket as everything else). WebSocket subscription to `/1.0/events` via `Client::subscribe_events`. |

`default = []` is explicit so Cargo feature unification stays a no-op for
downstream consumers who don't want the WebSocket dependency chain pulled
in. `cargo test -p incus-client` (default features, 67 tests) confirms
`tokio-tungstenite`/`futures` are absent from that build; `cargo test -p
incus-client --all-features` (72 tests) exercises the `events` feature too.

## API reference

### Client & configuration

- `ClientConfig::unix_socket(path)` - configure a client for a given socket
  path (e.g. `/var/lib/incus/unix.socket`). No I/O happens at construction;
  connections are made lazily, once per request.
- `ClientConfig::with_request_timeout(Option<Duration>)` - overrides the
  default 30-second per-request timeout. `None` disables it and waits
  indefinitely. This timeout applies to ordinary requests only - it does
  **not** apply to `wait_for_operation`'s long-poll call (see
  [Operations](#operations)).
- `Client::new(config)` - builds the client. `Client` is `Clone` and
  `Arc`-backed internally; share one instance across tasks rather than
  constructing a new one per call.

### Errors

Every fallible method returns `incus_client::Result<T>` (`Result<T,
Error>`). `Error` is `#[non_exhaustive]` (thiserror-derived) since this
crate expects to grow new failure modes as more of the API surface is
covered - always include a wildcard arm when matching.

| Variant | When |
|---|---|
| `Transport(io::Error)` | Connection, write, or read failure at the socket level. |
| `Api { status_code, message }` | Incus returned a 4xx/5xx response. `message` preserves the daemon's actual response body text wherever possible, even when it doesn't match the expected `{"error": "..."}` shape. |
| `Serialization(serde_json::Error)` | A request or response body failed to (de)serialize. |
| `InvalidResponse(String)` | The response didn't match any known Incus envelope shape, or a WebSocket frame couldn't be parsed as an event. |
| `ResponseTooLarge { limit }` | A response body, its cumulative headers, or a chunked-transfer chunk exceeded the 64 MiB cap (`transport::unix::MAX_RESPONSE_BYTES`). |
| `NotFound { resource }` | A `get_*`/`update_*`/`patch_*`/`delete_*` call got back HTTP 404. |
| `NotCancellable` | `cancel_operation` was called on an operation whose `may_cancel` is `false` - short-circuits with no network call. |
| `OperationFailed { id, status_code, err }` | `wait_for_operation` observed a terminal status in the 400-599 range. |
| `PreconditionFailed { resource }` | An `update_*`/`patch_*` call with an `etag` got back HTTP 412 - the ETag was stale. Re-fetch and retry. |
| `InvalidRequest(String)` | A caller-supplied path segment or `If-Match` value contained a control character (`\r`, `\n`, `?`, or `#`) and was rejected **before** anything was written to the socket - see [CRLF-injection protection](#reliability--hardening-behavior). |
| `Timeout { after, request_fully_sent }` | The per-request timeout elapsed (see `ClientConfig::with_request_timeout`). `request_fully_sent` tells you whether the request had already been fully written when the timeout fired - `true` means a mutating call may have already been received and acted on server-side even though you only see this error, so retrying could duplicate the operation; `false` means nothing was sent and a retry is safe. |
| `WebSocketProtocol(String)` (feature `events`) | A WebSocket-layer protocol violation from `/1.0/events` (oversized frame, malformed data, ...) - distinct from `Transport`, which is reserved for genuine socket I/O failures on that same stream. |

### Operations

Every mutation Incus documents as long-running returns an `Operation`
(`id: Uuid`, `class: OperationClass`, `status: String`, `status_code: u16`,
`resources`, `metadata`, `may_cancel: bool`, `err: Option<String>`) rather
than assuming synchronous completion. `OperationClass` has a
forward-compatible `Other(String)` variant for any value besides the three
Incus documents (`Task`, `Websocket`, `Token`) - deserializing an
unrecognized `class` doesn't fail the whole `Operation`.

- `Client::wait_for_operation(id, timeout: Option<Duration>) -> Result<Operation>`
  waits for operation `id` to reach a terminal status, using Incus's
  `.../wait?timeout=<seconds>` long-poll endpoint:
  - `timeout = Some(duration)` bounds a *single* long-poll call. If the
    operation is still in-progress when that window elapses, this returns
    `Ok(Operation)` with the in-progress snapshot - it does **not**
    re-poll, since the caller explicitly chose how long they're willing to
    wait.
  - `timeout = None` waits indefinitely by transparently re-issuing the
    long-poll call as many times as needed until a terminal status is
    reached. Each individual call is still bounded server-side; this just
    means the method as a whole doesn't return until completion. This path
    deliberately bypasses `ClientConfig`'s per-request default timeout (see
    [Reliability](#reliability--hardening-behavior)) so a legitimately slow
    operation (a large image import, say) isn't mistaken for a hang.
  - A terminal status in the 400-599 (failure) range returns
    `Err(Error::OperationFailed { .. })`, not `Ok(Operation)` - callers
    don't need to inspect `status_code` themselves to detect failure.
- `Client::cancel_operation(&op) -> Result<()>` cancels `op` if it's
  cancellable, short-circuiting with `Error::NotCancellable` (no network
  call) when `op.may_cancel` is `false`.

### Events (feature `events`)

WebSocket subscription to Incus's `/1.0/events` push-notification stream -
an enhancement over `wait_for_operation`, not a requirement for
operation-completion tracking.

- `EventFilter { operations, lifecycle, logging }` (all `true` by default)
  selects which event types to receive.
- `Client::subscribe_events(filter) -> Result<EventStream>` connects and
  returns a stream.
- `EventStream` implements `futures::Stream<Item = Result<Event>>`, yielding
  `Event::Operation(Operation)` (fully typed), `Event::Lifecycle(Value)`, or
  `Event::Logging(Value)` (untyped for v1 - only operation events need to be
  typed for completion tracking).
- The stream re-exposes the underlying WebSocket directly rather than
  buffering through an intermediate channel, so a slow consumer simply
  leaves frames unread in the transport's own receive buffer (natural
  backpressure) instead of risking unbounded in-process buffering.
- A malformed frame surfaces as one `Err` item rather than terminating the
  stream. A WebSocket close frame carrying a non-`Normal` code (daemon
  restart, internal error, protocol violation) also surfaces as one `Err`
  item - not silently treated the same as a graceful end-of-stream - and
  the stream is fused afterward so a caller can't observe the same
  terminal event twice.

### Resources

Every resource module follows the same shape: `list_*(recursion: bool)`,
`get_*`, `create_*`, `update_*`/`patch_*`, `delete_*`, all as methods on
`Client`. `list_*` methods return `Vec<serde_json::Value>` (not a typed
`Vec<Resource>`) because Incus's `recursion = false` mode returns bare
URL/name strings, not objects - only `recursion = true` returns full
objects, and the untyped return type tolerates both.

**Sync vs async is per-endpoint, not a blanket rule.** Every method's return
type reflects what the real Incus daemon actually does, verified against
`lxc/incus`'s `cmd/incusd/*.go` source (`main` branch) rather than assumed:
instance and image creation are genuinely long-running and return
`Operation` (wait on it with `wait_for_operation`); network, project, and
storage-pool create/update/delete are all synchronous in the real daemon
and return `Result<()>` directly, with nothing to wait for; storage volume
creation is the one *conditionally* sync-or-async endpoint in the API
(depends on whether the request copies another volume), so it returns
`Result<Option<Operation>>` - `None` means it already finished. An earlier
version of this crate assumed every mutation was async, which fails outright
against a real daemon for the sync endpoints (their response body has no
operation to parse) - see [Known limitations](#known-limitations--follow-up-work)
for how that was found and fixed.

**404 and 412 get typed errors.** Every `get_*`/`update_*`/`patch_*`/
`delete_*` method maps a 404 response to `Error::NotFound { resource }` and
(where `If-Match` applies) a 412 to `Error::PreconditionFailed { resource }`,
instead of the generic `Error::Api` a caller would otherwise have to inspect
`status_code` on.

**Instances** (`src/resources/instances.rs`) - containers and VMs:

| Method | Notes |
|---|---|
| `list_instances(recursion)` | |
| `get_instance(name)` | Returns `WithEtag<Instance>`. |
| `create_instance(&CreateInstanceParams)` | Returns `Operation` - genuinely async per Incus's documented behavior. `CreateInstanceParams { name, instance_type, source }` - `source` is raw JSON (`{"type": "image", "fingerprint": "..."}`, `"copy"`, `"migration"`, `"none"`). |
| `update_instance(name, &new_definition, etag)` | Returns `Operation`. Full replacement (PUT). |
| `update_instance_guarded(&WithEtag<Instance>, &new_definition)` | Same, but derives `name`/`etag` from a `get_instance` result directly - see [ETag / optimistic concurrency](#etag--optimistic-concurrency). |
| `patch_instance(name, &patch, etag)` | Returns `Operation`. Partial update (PATCH) - prefer this over `update_instance` for small config changes, to avoid a GET-then-PUT round trip. |
| `patch_instance_guarded(&WithEtag<Instance>, &patch)` | Guarded version of `patch_instance`. |
| `delete_instance(name)` | Returns `Operation`. |
| `start_instance` / `stop_instance` / `restart_instance` / `pause_instance` | Lifecycle actions via `PUT .../state`. Return `Operation`. |
| `list_snapshots(instance_name, recursion)` | |
| `create_snapshot(instance_name, snapshot_name)` | Returns `Operation`. |
| `delete_snapshot(instance_name, snapshot_name)` | Returns `Operation`. |

`Instance` fields: `name`, `status`, `status_code`, `instance_type`,
`architecture`, `created_at`, `last_used_at`, `location`, `project`,
`config` (untyped - Incus's instance config schema is large and mostly
free-form key-value pairs), `devices` (untyped), `profiles`.

**Images** (`src/resources/images.rs`):

| Method | Notes |
|---|---|
| `list_images(recursion)` | |
| `get_image(fingerprint)` | Returns `WithEtag<Image>`. |
| `create_image(&params)` | Returns `Operation` - genuinely async (fetching/unpacking a source, possibly a remote URL or a large upload). |
| `update_image(fingerprint, &new_definition, etag)` | Returns `Operation`. |
| `update_image_guarded(&WithEtag<Image>, &new_definition)` | Guarded version - see [ETag / optimistic concurrency](#etag--optimistic-concurrency). |
| `delete_image(fingerprint)` | Returns `Operation`. |

`Image` fields: `fingerprint`, `public`, `filename`, `size`,
`architecture`, `created_at`, `uploaded_at`, `properties` (untyped).

**Networks** (`src/resources/networks.rs`):

| Method | Notes |
|---|---|
| `list_networks(recursion)` | |
| `get_network(name)` | Returns `WithEtag<Network>`. |
| `create_network(&params)` | Returns `Result<()>` - synchronous, verified against `networksPost` in the real daemon source. |
| `update_network(name, &new_definition, etag)` | Returns `Result<()>` - synchronous. |
| `update_network_guarded(&WithEtag<Network>, &new_definition)` | Guarded version - see [ETag / optimistic concurrency](#etag--optimistic-concurrency). |
| `delete_network(name)` | Returns `Result<()>` - synchronous. |

`Network` fields: `name`, `network_type`, `managed`, `status`, `config`
(untyped).

**Projects** (`src/resources/projects.rs`):

| Method | Notes |
|---|---|
| `list_projects(recursion)` | |
| `get_project(name)` | Returns `WithEtag<Project>`. |
| `create_project(&params)` | Returns `Result<()>` - synchronous, verified against `projectsPost` in the real daemon source. |
| `update_project(name, &new_definition, etag)` | Returns `Result<()>` - synchronous. |
| `update_project_guarded(&WithEtag<Project>, &new_definition)` | Guarded version - see [ETag / optimistic concurrency](#etag--optimistic-concurrency). |
| `delete_project(name)` | Returns `Result<()>` - synchronous. |

`Project` fields: `name`, `description`, `config` (untyped).

**Storage pools & volumes** (`src/resources/storage.rs`) - volumes are
scoped under a pool; there is no global cross-pool volumes endpoint:

| Method | Notes |
|---|---|
| `list_storage_pools(recursion)` | |
| `get_storage_pool(name)` | Returns `WithEtag<StoragePool>`. |
| `create_storage_pool(&params)` | Returns `Result<()>` - synchronous, verified against `storagePoolsPost` in the real daemon source. |
| `update_storage_pool(name, &new_definition, etag)` | Returns `Result<()>` - synchronous. |
| `update_storage_pool_guarded(&WithEtag<StoragePool>, &new_definition)` | Guarded version - see [ETag / optimistic concurrency](#etag--optimistic-concurrency). |
| `delete_storage_pool(name)` | Returns `Result<()>` - synchronous. |
| `list_storage_volumes(pool_name, recursion)` | Scoped to one pool. |
| `create_storage_volume(pool_name, &params)` | Returns `Result<Option<Operation>>` - the one *conditionally* sync-or-async endpoint in this crate: `None` for a blank-volume create (no `source.name` in `params`), `Some(operation)` for a copy (`source.name` set), verified against `doVolumeCreateOrCopy` in the real daemon source. |
| `get_storage_volume(pool_name, volume_type, volume_name)` | Returns `WithEtag<StorageVolume>`. |
| `update_storage_volume(pool_name, volume_type, volume_name, &new_definition, etag)` | Returns `Result<()>` - synchronous. |
| `update_storage_volume_guarded(pool_name, &WithEtag<StorageVolume>, &new_definition)` | Guarded version - derives `volume_type` and the volume's own `name` from the fetched value; `pool_name` stays explicit since a volume's pool isn't part of its own returned object. |
| `delete_storage_volume(pool_name, volume_type, volume_name)` | Returns `Result<()>` - synchronous. |

`StoragePool` fields: `name`, `driver`, `status`, `config` (untyped).
`StorageVolume` fields: `name`, `volume_type`, `content_type`, `config`
(untyped).

### ETag / optimistic concurrency

Every resource type's `get_*` method - `get_instance`, `get_image`,
`get_network`, `get_project`, `get_storage_pool`, `get_storage_volume` -
returns `WithEtag<T>` instead of a bare `T`. Its fields are `pub(crate)`,
not `pub` - you can't construct one yourself, only receive one from a real
`get_*` call, which is what makes the ETag it carries trustworthy as
"this really was fetched" rather than typed in by hand. Read it back out
with `WithEtag::value()`, `WithEtag::etag()`, or `WithEtag::into_parts()`.

Two ways to use it:

- **Guarded (recommended):** pass the `WithEtag<T>` straight to the
  matching `update_*_guarded`/`patch_instance_guarded` method
  (`update_instance_guarded`, `patch_instance_guarded`,
  `update_image_guarded`, `update_network_guarded`,
  `update_project_guarded`, `update_storage_pool_guarded`,
  `update_storage_volume_guarded`) - it derives the resource identifier(s)
  and the `If-Match` value from the fetch for you.
- **Manual:** call `.etag()` yourself and pass it as the matching
  `update_*`/`patch_*` method's `etag: Option<&str>` parameter. This
  escape hatch stays available for a legitimate reason to supply your own
  ETag (e.g. one persisted from a previous process) - just note it's
  *not* type-checked against `WithEtag`, so nothing stops you from typing
  in a value that was never actually fetched.

Either way: if another caller changed the resource between your GET and
your PUT/PATCH, Incus returns HTTP 412 and this crate maps it to
`Err(Error::PreconditionFailed { resource })` - a distinct, matchable
variant, not a generic `Error::Api` you'd have to inspect `status_code` on.
Re-fetch and retry on that error.

## Reliability & hardening behavior

This crate went through two rounds of adversarial multi-agent review after
the initial implementation (plan review, then an 8-agent code review, then
a 6-agent PR review that caught a regression the first fix round
introduced). These are worth knowing about even though they're not
"limitations" per se:

- **Concurrency model.** Every request opens a fresh `UnixStream`
  connection rather than sharing/pooling one - connecting to a local Unix
  socket is cheap (no TCP handshake, no TLS negotiation), so this trivially
  guarantees concurrent requests never block each other, at the cost of
  one connect/close pair per request. `wait_for_operation`'s long-poll
  co-existing with other concurrent calls is exactly the scenario this
  design protects.
- **Defensive response caps, independently enforced at every layer.** A
  64 MiB cap (`transport::unix::MAX_RESPONSE_BYTES`) applies to: the
  response body (checked against `Content-Length` *before* allocating, or
  enforced incrementally for chunked/unbounded bodies); cumulative response
  header bytes *and* header count (100 max) - independent of the existing
  per-line cap, since an unbounded number of small, individually-legal
  header lines is its own DoS vector; and chunked-transfer chunk sizes,
  using `saturating_sub` rather than a bare addition so a peer-controlled
  chunk-size claim near `usize::MAX` can't overflow the check and bypass
  the cap.
- **CRLF-injection protection.** Every caller-supplied string that gets
  interpolated into a raw request path segment or the `If-Match` header
  value is validated for control characters (`\r`, `\n`, and bytes below
  0x20/0x7F) *before* any I/O happens - rejected with `Error::InvalidRequest`
  rather than risking HTTP request splitting against Incus's root-equivalent
  daemon socket. Query parameters are separately, correctly
  percent-encoded via `url::form_urlencoded`.
- **Per-request timeout, correctly scoped.** `ClientConfig`'s default
  30-second per-request timeout wraps ordinary requests' I/O phase only
  (not the pre-I/O validation above). `wait_for_operation`'s long-poll call
  explicitly bypasses this client-wide default and relies solely on its
  own server-side bound (Incus's `.../wait?timeout=` query param, or a
  genuinely unbounded long-poll when the caller passes `None`) - this was a
  real regression introduced mid-development and caught by independent
  review before merge; a regression test (`operations_tests.rs`) pins the
  correct behavior.
- **A blocking filesystem check never runs on the async executor thread.**
  The socket-path sanity check (`check_is_socket`) runs via
  `tokio::task::spawn_blocking` rather than inline, so it can't contend
  with other requests' progress on a loaded runtime.
- **No silent WebSocket termination.** See
  [Events](#events-feature-events) above - abnormal closes surface as an
  error, not a silent `None`.

## Testing

92 tests with `--all-features`, 86 with default features (the delta is the
`events` module's tests, feature-gated out by default). All are unit/
contract-level tests against a hand-rolled fake-daemon `UnixListener`
responder (`transport::unix::tests::spawn_fake_daemon`) - no live Incus
daemon is required or exercised anywhere in this suite. That's a real
limitation, not just a caveat: timing/concurrency quirks specific to a real
daemon aren't covered by CI. Where behavior couldn't be exercised against a
live daemon, it was instead verified by reading the actual `lxc/incus`
daemon source (`cmd/incusd/*.go` on `main`) - see
[Known limitations](#known-limitations--follow-up-work) for where that
mattered.

```bash
cargo test -p incus-client --all-features
cargo test -p incus-client                       # default features only
cargo clippy -p incus-client --all-targets --all-features -- -D warnings
cargo clippy -p incus-client --all-targets -- -D warnings
cargo fmt -p incus-client -- --check
```

Note the `--all-targets` on both clippy invocations: `cargo clippy -p
incus-client --all-features -- -D warnings` *without* `--all-targets` does
not compile `#[cfg(test)]` modules at all, and will silently miss lint
errors that only show up in test code - this bit the review process once
already.

## Known limitations / follow-up work

Beads epic `rmcp-template-hwu2` and all 30 of its child beads (the original
implementation tasks plus every finding from two full rounds of
multi-agent review, a targeted P3/P4 sweep, and the sync/async
verification below) are closed. What's actually still out of scope:

- **Remote mTLS/TOFU transport and certificates CRUD.** This crate is
  Unix-socket-only by design (see
  [What it deliberately doesn't do](#what-it-deliberately-doesnt-do)).
  Tracked separately as beads epic `rmcp-template-21b7`, pending a real
  remote consumer.
- **Sync-vs-async correctness has been verified against the real daemon
  source for every endpoint this crate calls, but not against a running
  daemon.** Every `create`/`update`/`delete` method's return type
  (`Result<()>`, `Result<Operation>`, or `Result<Option<Operation>>`) was
  checked against the actual response type each `cmd/incusd/*.go` handler
  returns on the `lxc/incus` `main` branch (see each method's doc comment
  for the exact function cited) - this caught a real bug: an earlier
  version of this crate assumed every mutation was async, which fails
  outright against a real daemon for the network/project/storage-pool
  endpoints (their sync response body has no operation to parse). Reading
  the source is strong evidence, but it isn't the same as exercising a
  live daemon end to end, which this crate's test suite doesn't do (see
  [Testing](#testing)).
- **`docs/superpowers/plans/2026-07-17-incus-client-crate.md`** (the
  original implementation plan, kept in the repo as a historical record)
  describes an earlier as-built snapshot of the transport layer that has
  since been rewritten (single buffered write instead of 3-4, buffered
  `read_until` instead of byte-by-byte, the request-timeout and
  `request_fully_sent` signal, sync/async corrections above, and more) -
  treat this README and the crate's own doc comments as authoritative,
  not that file.
