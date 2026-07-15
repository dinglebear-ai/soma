//! Two codegen passes, both driven by the vendored `schema/` assets:
//!
//! 1. `protocol.schema.json` (660 JSON Schema definitions) -> Rust types via `typify`.
//!    Written to `$OUT_DIR/protocol_generated.rs`, included by `src/protocol.rs`.
//! 2. `methods.json` (per-method name/params-type/response-type metadata, derived
//!    from the same schema by `cargo xtask codex-schema regen`) -> ergonomic
//!    per-method wrapper functions plus a small `impl ServerRequest` accessor
//!    block. Written to `$OUT_DIR/methods_generated.rs`, included by `src/client.rs`.
//!    Only the parts that genuinely vary per schema entry are generated here;
//!    static types built on top of them (e.g. `PendingServerRequest`) are
//!    hand-written in `src/client.rs` so they get normal Rust tooling
//!    (rustfmt, doc links, "jump to definition").
//!
//! Regenerating the schema assets (after bumping the installed `codex` CLI):
//! run `cargo xtask codex-schema regen <dir>` - see this crate's README
//! "Regenerating the schema" section for the full workflow. This build script
//! also does a best-effort staleness check (below): if a `codex` binary is on
//! `PATH` and its `--version` doesn't match the version stamped in
//! `schema/CODEX_VERSION.txt` at the last regen, it emits a non-fatal
//! `cargo:warning` - never a build failure, and never attempts to
//! regenerate anything itself.

use std::collections::BTreeSet;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

#[path = "src/build_support.rs"]
mod build_support;
use build_support::response_type_of;

fn main() {
    println!("cargo:rerun-if-changed=schema/protocol.schema.json");
    println!("cargo:rerun-if-changed=schema/methods.json");
    println!("cargo:rerun-if-changed=schema/CODEX_VERSION.txt");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    let out_dir = Path::new(&out_dir);

    generate_protocol_types(out_dir);
    generate_method_wrappers(out_dir);
    check_codex_staleness();
}

/// Best-effort, non-fatal staleness check: if `codex` is on `PATH` and its
/// reported version doesn't match `schema/CODEX_VERSION.txt` (stamped by the
/// last `cargo xtask codex-schema regen` run), warn that the vendored schema
/// may be out of date. This crate is a normal library dependency and must
/// build fine on machines/CI without `codex` installed at all, so a missing
/// binary (or any other lookup failure) is silently treated as "nothing to
/// warn about" - never a build failure, and no network/subprocess-driven
/// auto-regeneration is attempted here.
fn check_codex_staleness() {
    let Ok(installed) = Command::new("codex").arg("--version").output() else {
        return;
    };
    if !installed.status.success() {
        return;
    }
    let Ok(installed_version) = String::from_utf8(installed.stdout) else {
        return;
    };
    let installed_version = installed_version.trim();
    if installed_version.is_empty() {
        return;
    }

    let Ok(stamped) = fs::read_to_string("schema/CODEX_VERSION.txt") else {
        // No stamp yet (e.g. a checkout predating staleness tracking) -
        // nothing to compare against.
        return;
    };
    let stamped = stamped.trim();
    if stamped.is_empty() || stamped == installed_version {
        return;
    }

    println!(
        "cargo:warning=vendored codex-app-server-client schema was generated from `{stamped}`, but the installed `codex` CLI reports `{installed_version}`. The vendored schema may be stale - see README.md's 'Regenerating the schema' section (`cargo xtask codex-schema regen <dir>`)."
    );
}

fn generate_protocol_types(out_dir: &Path) {
    let content = fs::read_to_string("schema/protocol.schema.json")
        .expect("read schema/protocol.schema.json");
    let schema: schemars::schema::RootSchema = serde_json::from_str(&content)
        .expect("parse protocol.schema.json as a JSON Schema document");

    let mut settings = typify::TypeSpaceSettings::default();
    // `RequestId` (the JSON-RPC id, `string | int64`) is the one generated
    // type this crate needs to use as a `HashMap`/`HashSet` key - e.g. to
    // correlate in-flight server->client requests by id. Its variant
    // payloads (`String`, `i64`) are both natively `Eq`/`Hash`, so patching
    // in these derives *just* for this type is sound. A blanket
    // `.with_derive(...)` across all ~660 generated types would not be safe:
    // several of them embed `serde_json::Value` or `f64`, neither of which
    // implement `Eq`/`Hash`, and would fail to compile.
    let mut request_id_patch = typify::TypeSpacePatch::default();
    request_id_patch
        .with_derive("PartialEq")
        .with_derive("Eq")
        .with_derive("Hash");
    settings.with_patch("RequestId", &request_id_patch);

    let mut type_space = typify::TypeSpace::new(&settings);
    type_space
        .add_root_schema(schema)
        .expect("typify: convert protocol.schema.json to Rust types");

    let tokens = type_space.to_stream();
    write_formatted_rust(out_dir, "protocol_generated.rs", &tokens.to_string());
}

/// Parses `source` as a complete Rust file and pretty-prints it via
/// `prettyplease` before writing, matching typify's own discipline (see
/// `generate_protocol_types`) - a malformed manifest or template bug fails
/// loudly here, at generation time, rather than producing an unformatted
/// `.rs` file whose syntax errors only surface once `rustc` compiles the
/// `include!`d fragment deep inside `src/client.rs`.
fn write_formatted_rust(out_dir: &Path, file_name: &str, source: &str) {
    let file: syn::File = syn::parse_str(source).unwrap_or_else(|err| {
        panic!("generated {file_name} is not valid Rust: {err}\n\n---\n{source}\n---")
    });
    let formatted = prettyplease::unparse(&file);
    fs::write(out_dir.join(file_name), formatted)
        .unwrap_or_else(|err| panic!("write {file_name}: {err}"));
}

struct MethodEntry {
    method: String,
    variant_name: String,
    fn_name: String,
    params_type: Option<String>,
    params_optional: bool,
    response_type: Option<String>,
}

/// Reads a required string field from one `methods.json` entry, panicking
/// with the entry's index and the field name (not just a bare
/// `Option::unwrap()` message) so a malformed manifest points a maintainer
/// straight at the offending entry instead of an anonymous panic location.
fn required_str_field<'a>(e: &'a Value, index: usize, field: &str) -> &'a str {
    e[field].as_str().unwrap_or_else(|| {
        panic!("schema/methods.json entry #{index} is missing a string \"{field}\" field: {e}")
    })
}

fn parse_entries(value: &Value) -> Vec<MethodEntry> {
    value
        .as_array()
        .expect("methods.json section is an array")
        .iter()
        .enumerate()
        .map(|(index, e)| {
            let method = required_str_field(e, index, "method").to_string();
            MethodEntry {
                variant_name: required_str_field(e, index, "variant_name").to_string(),
                fn_name: required_str_field(e, index, "fn_name").to_string(),
                params_type: e["params_type"].as_str().map(str::to_string),
                params_optional: e["params_optional"].as_bool().unwrap_or(false),
                response_type: response_type_of(&method, e),
                method,
            }
        })
        .collect()
}

/// One (parameter signature, argument expression) pair, shared by every
/// per-method wrapper regardless of its response shape.
fn params_fragment(entry: &MethodEntry) -> (String, &'static str) {
    match (&entry.params_type, entry.params_optional) {
        (Some(pt), false) => (format!(", params: crate::protocol::{pt}"), "params"),
        (Some(pt), true) => (format!(", params: Option<crate::protocol::{pt}>"), "params"),
        (None, _) => (String::new(), "params: ()"),
    }
}

/// (return type, value-binding prefix, body-tail expression) triple, shared
/// by every per-method wrapper regardless of its params shape. The three
/// pieces are computed together (rather than `value_binding` being derived
/// separately from `entry.response_type.is_some()` elsewhere) so they can't
/// drift out of sync with each other - the binding and the tail expression
/// that consumes it must always agree on whether `value` exists.
fn response_fragment(entry: &MethodEntry) -> (String, &'static str, &'static str) {
    match &entry.response_type {
        Some(rt) => (
            format!("crate::Result<crate::protocol::{rt}>"),
            "let value = ",
            "Ok(serde_json::from_value(value)?)",
        ),
        None => ("crate::Result<()>".to_string(), "", "Ok(())"),
    }
}

fn generate_method_wrappers(out_dir: &Path) {
    let content = fs::read_to_string("schema/methods.json").expect("read schema/methods.json");
    let manifest: Value = serde_json::from_str(&content).expect("parse methods.json");

    let client_requests = parse_entries(&manifest["client_requests"]);
    let server_requests = parse_entries(&manifest["server_requests"]);
    let server_notifications = parse_entries(&manifest["server_notifications"]);

    assert_client_notifications_unchanged(&manifest["client_notifications"]);

    let mut out = String::new();
    writeln!(
        out,
        "// GENERATED by build.rs from schema/methods.json - do not edit by hand."
    )
    .unwrap();

    // -------- ergonomic per-method wrappers on CodexAppServerClient --------
    // One template for all 122 client-request methods: params/response shape
    // only ever varies along the two independent axes captured by
    // `params_fragment`/`response_fragment`, so there is exactly one place
    // to change wrapper behavior for every method at once (see the review
    // that flagged the prior 6-arm hand-duplicated version for drift risk -
    // two of those six arms weren't even reachable by the current schema).
    writeln!(out, "impl CodexAppServerClient {{").unwrap();
    for e in &client_requests {
        let doc = format!("Calls the `{}` app-server method.", e.method);
        let (param_sig, param_pass) = params_fragment(e);
        // When there's a response, `call_request(...).await?` binds to
        // `value` (via `value_binding`), which `ret_tail`
        // (`Ok(serde_json::from_value(value)?)`) consumes; when there isn't,
        // the call is just awaited for its error and `ret_tail` is a bare
        // `Ok(())`.
        let (ret_ty, value_binding, ret_tail) = response_fragment(e);
        let call_expr = format!(
            "{value_binding}self.call_request(|id| crate::protocol::ClientRequest::{variant} {{ id, {param_pass} }}).await?;",
            variant = e.variant_name,
        );
        writeln!(
            out,
            r#"    #[doc = {doc:?}]
    pub async fn {fn_name}(&self{param_sig}) -> {ret_ty} {{
        {call_expr}
        {ret_tail}
    }}"#,
            doc = doc,
            fn_name = e.fn_name,
        )
        .unwrap();
    }
    writeln!(out, "}}").unwrap();

    // -------- schema-derived accessors on ServerRequest --------
    // Only the two genuinely per-variant pieces (`id`, `method_name`) are
    // generated; `PendingServerRequest` itself (which wraps a `ServerRequest`
    // plus a reply channel, and exposes `respond`/`respond_error` on top of
    // these two accessors) is hand-written in `src/client.rs`.
    writeln!(
        out,
        r#"
impl crate::protocol::ServerRequest {{
    /// The `RequestId` the app-server expects echoed back in the reply.
    pub(crate) fn id(&self) -> &crate::protocol::RequestId {{
        match self {{"#
    )
    .unwrap();
    for e in &server_requests {
        writeln!(
            out,
            "            crate::protocol::ServerRequest::{variant} {{ id, .. }} => id,",
            variant = e.variant_name
        )
        .unwrap();
    }
    writeln!(
        out,
        r#"        }}
    }}

    /// The wire method name, e.g. `"execCommandApproval"`.
    pub(crate) fn method_name(&self) -> &'static str {{
        match self {{"#
    )
    .unwrap();
    for e in &server_requests {
        writeln!(
            out,
            "            crate::protocol::ServerRequest::{variant} {{ .. }} => {method:?},",
            variant = e.variant_name,
            method = e.method,
        )
        .unwrap();
    }
    writeln!(
        out,
        r#"        }}
    }}

    /// The name of the `crate::protocol` type `PendingServerRequest::respond`'s
    /// `result` must serialize to for this specific request. Not always
    /// `PascalCase(method_name) + "Response"` - a few methods need an
    /// irregular name (see `RESPONSE_OVERRIDES` in
    /// `xtask/src/codex_schema/naming.rs`) - so prefer this accessor over
    /// guessing the name yourself.
    pub(crate) fn expected_response_type_name(&self) -> &'static str {{
        match self {{"#
    )
    .unwrap();
    for e in &server_requests {
        let response_type = e.response_type.as_deref().unwrap_or_else(|| {
            panic!(
                "server request {} has no response type - every ServerRequest must expect a reply",
                e.method
            )
        });
        writeln!(
            out,
            "            crate::protocol::ServerRequest::{variant} {{ .. }} => {response_type:?},",
            variant = e.variant_name,
        )
        .unwrap();
    }
    writeln!(out, "        }}\n    }}\n}}").unwrap();

    // -------- schema-derived accessor on ServerNotification --------
    writeln!(
        out,
        r#"
impl crate::protocol::ServerNotification {{
    /// The wire method name, e.g. `"turn/completed"`. Useful for logging a
    /// non-sensitive identifier without the full (potentially large or
    /// sensitive) notification payload.
    pub fn method_name(&self) -> &'static str {{
        match self {{"#
    )
    .unwrap();
    for e in &server_notifications {
        writeln!(
            out,
            "            crate::protocol::ServerNotification::{variant} {{ .. }} => {method:?},",
            variant = e.variant_name,
            method = e.method,
        )
        .unwrap();
    }
    writeln!(out, "        }}\n    }}\n}}").unwrap();

    // -------- doc comment listing every notification variant handled by ServerNotification --------
    let notif_names: Vec<&str> = server_notifications
        .iter()
        .map(|e| e.method.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    writeln!(
        out,
        "\n/// All {n} server notification methods this crate can decode: {list:?}",
        n = notif_names.len(),
        list = notif_names,
    )
    .unwrap();
    writeln!(
        out,
        "pub const SERVER_NOTIFICATION_METHODS: &[&str] = &{notif:?};",
        notif = notif_names
    )
    .unwrap();

    write_formatted_rust(out_dir, "methods_generated.rs", &out);
}

/// `client_notifications` (client->server, fire-and-forget messages) has
/// exactly one entry today - `"initialized"` - which is hand-sent by
/// `CodexAppServerClient::send_initialized` in `src/client.rs` (bypassing the
/// generated `ClientNotification` type, which typify represents oddly for a
/// single-variant `oneOf`; see that method's doc comment). If a future
/// `codex` schema version adds a second client notification, this manifest
/// section would silently grow with no wrapper method and no way to send it -
/// fail the build loudly instead so a maintainer notices and adds a
/// hand-written sender for it (mirroring `send_initialized`), then updates
/// this assertion.
fn assert_client_notifications_unchanged(client_notifications: &Value) {
    let methods: Vec<&str> = client_notifications
        .as_array()
        .expect("methods.json client_notifications is an array")
        .iter()
        .map(|e| e["method"].as_str().unwrap())
        .collect();
    assert_eq!(
        methods,
        vec!["initialized"],
        "schema/methods.json's client_notifications changed from the one expected entry \
         (\"initialized\"). This crate has no generic way to send client notifications - add a \
         hand-written sender in src/client.rs for the new method (see send_initialized), then \
         update this assertion in build.rs to match the new expected list."
    );
}
