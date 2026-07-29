//! OpenAPI 3.1.0 document for the `rest` module's HTTP surface.
//!
//! [`openapi_spec`] hand-builds a `serde_json::Value` describing every route
//! in [`super::routes`] rather than deriving it from the Rust types via a
//! schema-generation crate (`schemars`, `utoipa`, ...). That's a deliberate
//! trade-off, not an oversight: this crate promises zero path-dependencies
//! and a minimal, audited `crates.io` dependency footprint (see
//! `README.md`), and every schema-derive crate considered pulls in either a
//! proc-macro-heavy dependency tree or its own opinionated JSON Schema
//! dialect that doesn't map onto OpenAPI 3.1 cleanly (`serde`'s
//! `rename_all_fields` internally-tagged enums in particular - see
//! [`RestEventResponse`](super::types::RestEventResponse) - have no clean
//! derive-crate story as of this writing). Hand-writing means every schema
//! below is transcribed by reading `src/rest/types.rs` and `src/compat.rs`
//! directly; the tests at the bottom of this file exist specifically to
//! catch that transcription drifting from the real wire format or the real
//! mounted routes.
//!
//! # Determinism
//!
//! This crate does not enable `serde_json`'s `preserve_order` feature, so in
//! an ordinary standalone build `serde_json::Map` is backed by a
//! `BTreeMap` and serializes with sorted keys automatically. But Cargo
//! unifies feature flags across an entire build's unit graph: when this
//! crate is built as part of the full `soma` workspace (rather than in
//! isolation with `cargo test -p codex-app-server-client`), sibling crates
//! that *do* enable `preserve_order` (see `crates/shared/openapi/Cargo.toml`,
//! `crates/shared/codemode/Cargo.toml`) flip `serde_json::Map` to an
//! insertion-order-preserving `IndexMap` for every crate in that build,
//! including this one - see `xtask/Cargo.toml`'s comment on the same
//! incident for prior art. `json::obj` below builds every JSON object in
//! this module by sorting its entries before insertion, so
//! [`openapi_spec`]'s serialized output is byte-identical either way.
//! [`serde_json::json!`] is still used freely for arrays and scalar leaves,
//! where element order is meaningful (arrays) or there's nothing to order
//! (leaves) - only object-shaped literals go through `json::obj`.
//!
//! # Module layout
//!
//! This document has three natural pieces, plus the shared low-level JSON
//! builders they're all built from:
//!
//! - [`json`] - generic, schema-agnostic JSON/JSON-Schema value builders
//!   (`obj`, `schema_ref`, `object_schema`, ...).
//! - [`schemas`] - `components.schemas`, i.e. [`schemas::build_schemas`].
//! - [`route_table`] - the single source-of-truth route table ([`route_table::ROUTES`])
//!   read by both [`paths`] and this file's own coverage tests.
//! - [`paths`] - `paths`, i.e. [`paths::build_paths`], including every
//!   per-operation request/response/parameter builder.

use serde_json::{Value, json};

mod json;
mod paths;
mod route_table;
mod schemas;

#[cfg(test)]
use route_table::{ROUTES, RouteDef};

/// Builds the full OpenAPI 3.1.0 document for the `rest` module.
///
/// Deterministic: every object in the returned [`serde_json::Value`] is
/// built through `json::obj`, so `serde_json::to_string_pretty(&openapi_spec())`
/// is byte-identical run to run and build to build - see the module docs'
/// "Determinism" section. `tests::openapi_spec_matches_checked_in_file`
/// pins that output against the checked-in `openapi.json`.
pub fn openapi_spec() -> Value {
    json::obj(vec![
        ("openapi", json!("3.1.0")),
        (
            "info",
            json::obj(vec![
                ("title", json!("codex-app-server-client REST adapter")),
                ("version", json!(env!("CARGO_PKG_VERSION"))),
                (
                    "description",
                    json!(
                        "HTTP surface for the optional `rest` feature of `codex-app-server-client` \
                         - a portable adapter around local `codex app-server` JSON-RPC processes. \
                         This is only an adapter: it does not authenticate callers, authorize \
                         requests, sandbox clients, or make the upstream app-server safe to expose \
                         on a network by itself.\n\n\
                         **Routes are opt-in per router constructor** - see each operation's \
                         description for which of `rest::router()` (health/compat only), \
                         `rest::text_turn_router()`, or `rest::trusted_bridge_router()` mounts it. \
                         `rest::router_with_options`/`_with_backend*` let a host application mix \
                         and match via `RestRouterOptions`.\n\n\
                         **Authentication is opt-in and not part of the base router.** Wrap any \
                         router in `rest::bearer_auth(token)` (a `tower` `Layer`) to require an \
                         `Authorization: Bearer <token>` header on every request except (by \
                         default) `GET /health` and `GET /v1/health` - see \
                         `BearerAuthLayer::allow_unauthenticated_health`. Operations below that can \
                         return `401` note that it only applies once this layer is added; the base \
                         router never returns `401` on its own. A caller that presents the one \
                         configured token gets everything the mounted router exposes - this is \
                         transport auth only, not per-session or per-method authorization."
                    ),
                ),
            ]),
        ),
        (
            "servers",
            json!([json::obj(vec![
                ("url", json!("http://127.0.0.1:43210")),
                (
                    "description",
                    json!(
                        "Default loopback bind address used by the `rest_server` example and the `codex-app-server-rest` binary's `text-turn` mode."
                    )
                ),
            ]),]),
        ),
        (
            "components",
            json::obj(vec![
                (
                    "securitySchemes",
                    json::obj(vec![(
                        "bearerAuth",
                        json::obj(vec![
                            ("type", json!("http")),
                            ("scheme", json!("bearer")),
                            (
                                "description",
                                json!(
                                    "Opt-in via `rest::bearer_auth(token)`; not required by the base \
                                     router. See this document's top-level `info.description`."
                                ),
                            ),
                        ]),
                    )]),
                ),
                ("schemas", schemas::build_schemas()),
            ]),
        ),
        ("paths", paths::build_paths()),
    ])
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs, path::PathBuf};

    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request, header},
    };
    use tower::ServiceExt;

    use super::*;
    use crate::{
        CompatibilityReport,
        rest::{
            RestBackend, RestFuture, RestRouterOptions, RestTextTurnResponse,
            router_with_backend_and_options,
        },
    };

    /// Path to the checked-in spec, relative to this crate's manifest.
    fn checked_in_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("openapi.json")
    }

    fn rendered() -> String {
        // `to_string_pretty` (not `to_string`) so the checked-in file is
        // human-diffable in review, same rationale as any other checked-in
        // generated JSON in this repo.
        let mut text = serde_json::to_string_pretty(&openapi_spec())
            .expect("openapi_spec() must always serialize");
        text.push('\n');
        text
    }

    /// Pins `openapi_spec()`'s serialized output against the checked-in
    /// `openapi.json`. This crate has no `xtask` (see README.md: zero
    /// path-dependencies on anything else in the workspace, including
    /// tooling crates), so unlike `docs/generated/openapi.json` elsewhere in
    /// this repo, regeneration is an env-var-gated test run rather than an
    /// `xtask` subcommand:
    ///
    /// ```sh
    /// CODEX_REST_OPENAPI_WRITE=1 cargo test -p codex-app-server-client --features rest openapi_spec_matches_checked_in_file
    /// ```
    ///
    /// then review the diff and commit `openapi.json` alongside the change
    /// that caused it.
    #[test]
    fn openapi_spec_matches_checked_in_file() {
        let rendered = rendered();
        if std::env::var_os("CODEX_REST_OPENAPI_WRITE").is_some() {
            fs::write(checked_in_path(), &rendered).expect("failed to write openapi.json");
            return;
        }
        let checked_in = fs::read_to_string(checked_in_path()).unwrap_or_else(|error| {
            panic!(
                "failed to read {}: {error}\n\n\
                 Generate it with:\n\
                 CODEX_REST_OPENAPI_WRITE=1 cargo test -p codex-app-server-client --features rest openapi_spec_matches_checked_in_file",
                checked_in_path().display()
            )
        });
        // Compare line-by-line rather than byte-for-byte: git checks this file
        // out with CRLF on Windows (`core.autocrlf`), while `rendered()`
        // always emits LF, so a byte compare fails there for a reason that has
        // nothing to do with the spec's content. Same approach as
        // `apps/soma/tests/architecture_boundaries.rs`, which hit this first.
        // Still an exact comparison of every line, so real drift - a changed
        // value, a added or removed key - fails exactly as before.
        assert!(
            rendered.lines().eq(checked_in.lines()),
            "openapi_spec() no longer matches the checked-in openapi.json.\n\n\
             Regenerate it with:\n\
             CODEX_REST_OPENAPI_WRITE=1 cargo test -p codex-app-server-client --features rest openapi_spec_matches_checked_in_file\n\n\
             then review the diff and commit crates/shared/codex-app-server-client/openapi.json."
        );
    }

    /// Every `$ref` and `discriminator.mapping` target in the document must
    /// name a schema that actually exists under `components/schemas`.
    ///
    /// This exists because it caught a real bug: `RestEventResponse`'s
    /// `discriminator.mapping` pointed at four `RestEventResponse*` names
    /// whose schemas were only ever built inline inside the `oneOf` array and
    /// never registered as components. The document still round-tripped as
    /// JSON and every other test passed - but a spec-compliant generator
    /// (`openapi-typescript`, via Redocly) rejects the whole document when it
    /// can't resolve a mapping ref, which defeats the point of publishing the
    /// spec at all. A dangling ref is invisible to `serde_json`, so it needs
    /// its own assertion.
    /// Every operation that declares a `requestBody` must document a `413`.
    ///
    /// The router caps request bodies with `DefaultBodyLimit`
    /// (`RestLimits::max_request_body_bytes`), so any body-reading route can
    /// return `413` before its handler runs. `paths::ensure_request_body_limit_413`
    /// adds it centrally; this asserts nothing slipped past that - a route with
    /// a body but no documented `413` would under-report the real API to every
    /// generated client.
    #[test]
    fn every_route_with_a_request_body_documents_413() {
        let spec = openapi_spec();
        let paths = spec["paths"].as_object().expect("paths is an object");

        let mut missing = Vec::new();
        for (path, item) in paths {
            let operations = item.as_object().expect("path item is an object");
            for (method, operation) in operations {
                if operation.get("requestBody").is_none() {
                    continue;
                }
                if operation["responses"].get("413").is_none() {
                    missing.push(format!("{} {path}", method.to_uppercase()));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "these operations declare a requestBody but do not document a 413 (the router's \
             DefaultBodyLimit can reject any of them): {missing:?}"
        );
    }

    #[test]
    fn every_schema_ref_resolves_to_a_real_component() {
        let spec = openapi_spec();
        let defined: BTreeSet<String> = spec["components"]["schemas"]
            .as_object()
            .expect("components/schemas is an object")
            .keys()
            .cloned()
            .collect();

        let mut refs = Vec::new();
        collect_schema_refs(&spec, &mut refs);
        assert!(
            !refs.is_empty(),
            "found no schema refs at all - collect_schema_refs is not walking the document"
        );

        let dangling: Vec<&String> = refs
            .iter()
            .filter(|name| !defined.contains(*name))
            .collect();
        assert!(
            dangling.is_empty(),
            "openapi_spec() references schemas that do not exist under components/schemas: \
             {dangling:?}\n\nDefined schemas: {defined:?}"
        );
    }

    /// Walks the whole document collecting every `#/components/schemas/<name>`
    /// target, from both `$ref` values and `discriminator.mapping` values
    /// (the latter are plain strings, not `$ref` objects, which is exactly why
    /// they were able to dangle unnoticed).
    fn collect_schema_refs(value: &Value, out: &mut Vec<String>) {
        const PREFIX: &str = "#/components/schemas/";
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    match (key.as_str(), child) {
                        ("$ref", Value::String(target)) => {
                            if let Some(name) = target.strip_prefix(PREFIX) {
                                out.push(name.to_owned());
                            }
                        }
                        ("mapping", Value::Object(mapping)) => {
                            for target in mapping.values().filter_map(Value::as_str) {
                                if let Some(name) = target.strip_prefix(PREFIX) {
                                    out.push(name.to_owned());
                                }
                            }
                        }
                        _ => collect_schema_refs(child, out),
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    collect_schema_refs(item, out);
                }
            }
            _ => {}
        }
    }

    /// A `RestBackend` that answers every call immediately without spawning
    /// a real `codex` process - the two mandatory trait methods
    /// (`compatibility_report`, `run_text_turn`) get trivial canned
    /// responses; every other method falls back to the trait's own default
    /// impl (see `src/rest/types.rs`), which already returns a
    /// properly-JSON-shaped `RestError::NotFound`/empty-list/etc for
    /// anything it doesn't implement - exactly the shape a real backend
    /// would return for "session not found", which is what
    /// `every_documented_route_is_actually_mounted` needs: a JSON response
    /// distinguishable from axum's own no-route-matched fallback, without
    /// depending on `codex` being installed in the test environment.
    struct MinimalBackend;

    impl RestBackend for MinimalBackend {
        fn compatibility_report(&self) -> RestFuture<CompatibilityReport> {
            Box::pin(async { Ok(CompatibilityReport::from_installed_version(None)) })
        }

        fn run_text_turn(
            &self,
            _request: super::super::types::RestTextTurnRequest,
        ) -> RestFuture<RestTextTurnResponse> {
            Box::pin(async { Ok(RestTextTurnResponse::default()) })
        }
    }

    /// Request bodies for the routes that require one. Kept next to (not
    /// merged into) [`ROUTES`] because a body is a probe-test concern only -
    /// `openapi_spec()` itself never needs a concrete instance, only the
    /// schema.
    fn probe_body(route: &RouteDef) -> Option<Value> {
        match (route.method, route.path_template) {
            ("post", "/v1/text-turn") => Some(json!({"prompt": "hello"})),
            ("post", "/v1/call/{method}") => Some(json!({})),
            ("post", "/v1/sessions") => Some(json!({})),
            ("post", "/v1/sessions/{sessionId}/call/{method}") => Some(json!({})),
            ("post", "/v1/sessions/{sessionId}/requests/{requestKey}/result") => {
                Some(json!({"result": {}}))
            }
            ("post", "/v1/sessions/{sessionId}/requests/{requestKey}/error") => {
                Some(json!({"code": -32000, "message": "denied"}))
            }
            _ => None,
        }
    }

    /// The coverage test for bead g0qf.2's "route-coverage" requirement.
    ///
    /// axum 0.8's `Router` has no public API to enumerate its own routes, so
    /// this can't diff "routes the live router actually has" against
    /// "routes `openapi_spec()` documents" by introspection. Instead it
    /// takes the documented, honest fallback: [`ROUTES`] is the *one* table
    /// both `openapi_spec()` (via [`paths::build_paths`]) and this
    /// test read, and this test proves each entry in it is real by actually
    /// issuing an HTTP request for it against a live
    /// `trusted_bridge_router()` (the superset router - it mounts every gate
    /// in `RouteGate`) and checking the response could only have come from
    /// that route's real handler, not axum's built-in no-route-matched
    /// fallback:
    ///
    /// - every route except the SSE stream one always answers with a
    ///   `Content-Type: application/json` body on this crate's own success
    ///   *and* error paths (including extraction failures like a malformed
    ///   body - see `invalid_json`/`invalid_request` in `routes.rs`, both of
    ///   which still go through `Json(...)`), so any non-JSON body is proof
    ///   the request never reached a real handler.
    /// - the SSE stream route commits `Content-Type: text/event-stream` the
    ///   moment the response starts, before the backend is even polled once
    ///   (see that operation's description above), so a non-SSE content
    ///   type there is the same tell.
    ///
    /// What this test does *not* catch: a route added to `routes.rs` and
    /// never added to `ROUTES` here. axum exposes no route-enumeration API,
    /// so that direction is unverifiable *from the live router*.
    /// [`every_path_mounted_by_routes_rs_is_in_the_routes_table`] covers it
    /// from the other side instead, by reading `routes.rs`'s own source for
    /// its `.route(...)` path literals - the two tests together pin both
    /// directions.
    #[tokio::test]
    async fn every_documented_route_is_actually_mounted() {
        let app =
            router_with_backend_and_options(MinimalBackend, RestRouterOptions::trusted_bridge());

        for route in ROUTES {
            let method = Method::from_bytes(route.method.to_ascii_uppercase().as_bytes())
                .unwrap_or_else(|error| panic!("invalid method `{}`: {error}", route.method));
            let mut builder = Request::builder().method(method).uri(route.probe_path);
            let request = match probe_body(route) {
                Some(body) => {
                    builder = builder.header(header::CONTENT_TYPE, "application/json");
                    builder.body(Body::from(body.to_string())).unwrap()
                }
                None => builder.body(Body::empty()).unwrap(),
            };

            let response = app.clone().oneshot(request).await.unwrap_or_else(|error| {
                panic!("{} {} failed: {error}", route.method, route.probe_path)
            });
            let content_type = response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            let status = response.status();
            // Drain the body so a hung/oversized stream would fail the test
            // rather than the test process, and so `content_type` above
            // (already read from headers, which arrive before any body) is
            // the only thing this assertion needs.
            let _ = to_bytes(response.into_body(), usize::MAX).await;

            let is_sse_route = route.path_template.ends_with("/events/stream");
            if is_sse_route {
                assert!(
                    content_type.starts_with("text/event-stream"),
                    "{} {} returned Content-Type {content_type:?} (status {status}), not \
                     text/event-stream - openapi_spec() documents this as the SSE route but \
                     trusted_bridge_router() did not route the probe request to it",
                    route.method,
                    route.probe_path,
                );
            } else {
                assert!(
                    content_type.starts_with("application/json"),
                    "{} {} returned Content-Type {content_type:?} (status {status}), not JSON - \
                     openapi_spec() documents `{} {}` as a mounted route, but \
                     trusted_bridge_router() did not route the probe request to a real handler \
                     (axum's own no-route-matched fallback never returns application/json)",
                    route.method,
                    route.probe_path,
                    route.method,
                    route.path_template,
                );
            }
        }
    }

    /// Every path mounted by `routes.rs` must appear in [`ROUTES`], and vice
    /// versa.
    ///
    /// This closes the one direction
    /// [`every_documented_route_is_actually_mounted`] structurally cannot:
    /// that test probes a live router for each `ROUTES` entry, proving
    /// `ROUTES` is a subset of what is mounted. A route added to `routes.rs`
    /// and never mirrored here would compile, pass every other test, and be
    /// silently absent from `openapi.json` and every generated client
    /// forever - the failure mode with no symptom.
    ///
    /// Works by reading `routes.rs`'s own source and extracting its
    /// `.route("...")` path literals. Source-grep rather than introspection
    /// because axum 0.8's `Router` exposes no route-enumeration API (the same
    /// reason `ROUTES` exists at all), and this repo already uses
    /// source-pattern contract checks elsewhere (`xtask/src/patterns/`).
    ///
    /// Compares paths only, not methods: the two files spell parameters
    /// differently (`routes.rs` uses axum's `{session_id}`/`{*method}`, this
    /// table uses OpenAPI's `{sessionId}`), so both sides are normalized to a
    /// bare `{}` placeholder per parameter segment. A residual gap that
    /// remains: adding a *method* to an already-listed path (e.g. a `.patch()`
    /// on `/v1/sessions`) is not caught here - only a new or removed path is.
    #[test]
    fn every_path_mounted_by_routes_rs_is_in_the_routes_table() {
        /// Collapses each `{...}` path segment to `{}` so axum's and
        /// OpenAPI's differing parameter spellings compare equal.
        fn normalize(path: &str) -> String {
            path.split('/')
                .map(|segment| {
                    if segment.starts_with('{') && segment.ends_with('}') {
                        "{}"
                    } else {
                        segment
                    }
                })
                .collect::<Vec<_>>()
                .join("/")
        }

        let source = include_str!("routes.rs");
        let call_count = source.matches(".route(").count();
        let mounted: BTreeSet<String> = source
            .match_indices(".route(")
            .map(|(index, matched)| {
                // `rustfmt` wraps longer calls, so the path literal is not
                // necessarily on the same line as `.route(`. Skip whitespace
                // rather than assuming `.route("`, and fail loudly on any
                // shape this parser cannot read - a silently-skipped call
                // would make this whole test vacuously pass.
                let rest = source[index + matched.len()..].trim_start();
                let rest = rest.strip_prefix('"').unwrap_or_else(|| {
                    panic!(
                        "expected a string literal as the first argument to .route(, found: {:?}. \
                         This test parses routes.rs's source; teach it the new shape rather than \
                         letting it skip the call.",
                        &rest[..rest.len().min(40)]
                    )
                });
                let end = rest
                    .find('"')
                    .expect("a .route( path literal must have a closing quote");
                normalize(&rest[..end])
            })
            .collect();
        assert!(
            !mounted.is_empty(),
            "found no .route(...) calls in routes.rs - the source-grep in this test has broken \
             and is silently proving nothing"
        );
        // `mounted` is a set, so duplicate paths would collapse silently.
        // There are none today; if that changes, this catches the collapse
        // rather than letting the set comparison quietly under-count.
        assert_eq!(
            mounted.len(),
            call_count,
            "routes.rs has {call_count} .route(...) calls but only {} distinct paths - a \
             duplicate path would make the comparison below under-count",
            mounted.len()
        );

        let documented: BTreeSet<String> = ROUTES
            .iter()
            .map(|route| normalize(route.path_template))
            .collect();

        let undocumented: Vec<&String> = mounted.difference(&documented).collect();
        assert!(
            undocumented.is_empty(),
            "routes.rs mounts these paths, but they are missing from ROUTES - they would be \
             absent from openapi.json and every generated client: {undocumented:?}"
        );

        let unmounted: Vec<&String> = documented.difference(&mounted).collect();
        assert!(
            unmounted.is_empty(),
            "ROUTES documents these paths, but routes.rs does not mount them: {unmounted:?}"
        );
    }

    /// Cheap structural sanity check that every [`ROUTES`] entry has a
    /// matching `paths.<template>.<method>` entry in `openapi_spec()`, and
    /// vice versa. This is *not* the coverage test bead g0qf.2 asks for -
    /// [`every_documented_route_is_actually_mounted`] above is - since
    /// `build_paths` mechanically derives `paths` from `ROUTES`, so by
    /// construction this can only fail if `operation_for`'s match arms and
    /// `ROUTES`'s entries fall out of sync with each other (a bug this
    /// module could introduce internally), not if `routes.rs` drifts from
    /// either. Kept anyway because it's a real, cheap regression guard for
    /// that internal-consistency failure mode, and its assertion messages
    /// are far more direct than tracing a panic out of `operation_for`.
    #[test]
    fn openapi_spec_paths_match_routes_table_exactly() {
        let spec = openapi_spec();
        let paths = spec
            .get("paths")
            .and_then(Value::as_object)
            .expect("openapi_spec() must have an object `paths`");

        for route in ROUTES {
            let path_item = paths.get(route.path_template).unwrap_or_else(|| {
                panic!(
                    "ROUTES has `{} {}` but openapi_spec()'s paths has no entry for `{}`",
                    route.method, route.probe_path, route.path_template
                )
            });
            assert!(
                path_item.get(route.method).is_some(),
                "ROUTES has `{} {}` but openapi_spec()'s path item for `{}` has no `{}` operation",
                route.method,
                route.probe_path,
                route.path_template,
                route.method,
            );
        }

        for (path, item) in paths {
            let methods = item.as_object().unwrap_or_else(|| {
                panic!("openapi_spec()'s path item for `{path}` is not an object")
            });
            for method in methods.keys() {
                assert!(
                    ROUTES
                        .iter()
                        .any(|route| route.path_template == path && route.method == method),
                    "openapi_spec() documents `{method} {path}` but ROUTES has no matching entry"
                );
            }
        }
    }
}
