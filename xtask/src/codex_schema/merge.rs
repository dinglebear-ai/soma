//! Core schema-merging logic - a faithful Rust port of the definitions-
//! combining half of the former `schema/build_combined_schema.py` (see git
//! history for the original). Reused by both `regen` (the real merge) and
//! `bisect` (building opaqued-out candidate schemas from the same merge).

use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

use super::naming;

/// Recursively rewrites `"$ref": "#/definitions/v2/X"` to `"#/definitions/X"`
/// so refs resolve against a single flat `definitions` namespace.
pub fn rewrite_v2_refs(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            for (k, v) in map {
                if k == "$ref" {
                    if let Value::String(s) = v {
                        if let Some(rest) = s.strip_prefix("#/definitions/v2/") {
                            out.insert(k.clone(), Value::String(format!("#/definitions/{rest}")));
                            continue;
                        }
                    }
                }
                out.insert(k.clone(), rewrite_v2_refs(v));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(rewrite_v2_refs).collect()),
        other => other.clone(),
    }
}

/// The `McpServerElicitationRequestParams` typify-0.7.0 workaround: merges a
/// schema's top-level object `properties`/`required` into each sibling
/// `oneOf` branch, producing a pure oneOf-of-self-contained-objects.
/// Returns the schema unchanged unless it actually has this exact shape
/// (top-level `oneOf` and `properties` both present).
///
/// Every `oneOf` branch must itself be an inline `{"type": "object", ...}`
/// schema with a `properties` object (`required` may be absent, meaning no
/// branch-specific required fields) - bails otherwise rather than silently
/// treating an unrecognized branch shape (e.g. a bare `$ref` to a named
/// branch type) as if it contributed no fields. Silently defaulting a branch
/// to empty would let `typify` accept the resulting schema while silently
/// dropping every field that branch's `$ref` would have contributed, with no
/// build failure.
pub fn flatten_base_plus_oneof(schema: &Value) -> Result<Value> {
    let Some(obj) = schema.as_object() else {
        return Ok(schema.clone());
    };
    if !(obj.contains_key("oneOf") && obj.contains_key("properties")) {
        return Ok(schema.clone());
    }

    let base_props = obj
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let base_required: BTreeSet<String> = obj
        .get("required")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let one_of = obj
        .get("oneOf")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut flattened = Vec::with_capacity(one_of.len());
    for (i, branch) in one_of.iter().enumerate() {
        let branch_obj = branch.as_object().with_context(|| {
            format!(
                "flatten_base_plus_oneof: oneOf branch {i} is not an inline object schema ({branch}) \
                 - this workaround only knows how to flatten inline `{{\"type\": \"object\", ...}}` \
                 branches. Update flatten_base_plus_oneof to handle this shape (e.g. a $ref branch)."
            )
        })?;
        if branch_obj.get("type").and_then(Value::as_str) != Some("object") {
            bail!(
                "flatten_base_plus_oneof: oneOf branch {i} is not \"type\": \"object\" ({branch}) - \
                 refusing to flatten a branch shape this workaround wasn't written for."
            );
        }
        if !branch_obj.contains_key("properties") {
            bail!(
                "flatten_base_plus_oneof: oneOf branch {i} has no \"properties\" key ({branch}) - \
                 refusing to flatten, since that would silently contribute zero fields from this \
                 branch."
            );
        }

        // {**base_props, **branch.properties}: base first (its order), then
        // branch entries override/append - matches Python dict-merge.
        let mut merged_props = base_props.clone();
        if let Some(bp) = branch_obj.get("properties").and_then(Value::as_object) {
            for (k, v) in bp {
                merged_props.insert(k.clone(), v.clone());
            }
        }

        // sorted(set(base_required) | set(branch_required))
        let mut merged_required = base_required.clone();
        if let Some(br) = branch_obj.get("required").and_then(Value::as_array) {
            merged_required.extend(br.iter().filter_map(|v| v.as_str().map(String::from)));
        }

        let mut branch_out = Map::new();
        branch_out.insert("type".to_string(), json!("object"));
        branch_out.insert("properties".to_string(), Value::Object(merged_props));
        branch_out.insert(
            "required".to_string(),
            Value::Array(merged_required.into_iter().map(Value::String).collect()),
        );
        flattened.push(Value::Object(branch_out));
    }

    let mut out = Map::new();
    out.insert(
        "title".to_string(),
        obj.get("title").cloned().unwrap_or(Value::Null),
    );
    out.insert("oneOf".to_string(), Value::Array(flattened));
    Ok(Value::Object(out))
}

/// Definition names known to legitimately differ between the master and v2
/// bundles - v2's copy is the deliberately-pruned/authoritative one and is
/// *expected* to differ from master's (ref-rewritten) copy. Any other
/// name-collision with differing content is treated as a hard failure (see
/// `check_collision_compatibility`) rather than silently letting v2 shadow a
/// master definition that might carry different meaning.
const EXPECTED_DIVERGENT_COLLISIONS: &[&str] =
    &["ClientRequest", "ServerNotification", "RequestId"];

/// For every definition name present in both `master_flat_rewritten` and
/// `v2_defs`, verifies they're either identical or on the explicit
/// known-divergent allowlist. `build_combined` always prefers v2's copy on a
/// collision; this only exists to catch collisions where that's *not* safe -
/// a future codex schema change that makes some other same-named type
/// genuinely diverge between the two bundles would otherwise have v2 silently
/// substitute the wrong shape into a master-derived envelope type (several of
/// which transitively `$ref` into collision names like `AbsolutePathBuf` and
/// `RequestId`), with no build failure and no warning.
fn check_collision_compatibility(
    master_flat_rewritten: &Map<String, Value>,
    v2_defs: &Map<String, Value>,
) -> Result<()> {
    for (name, master_def) in master_flat_rewritten {
        let Some(v2_def) = v2_defs.get(name) else {
            continue;
        };
        if master_def == v2_def {
            continue;
        }
        if EXPECTED_DIVERGENT_COLLISIONS.contains(&name.as_str()) {
            continue;
        }
        bail!(
            "build_combined: definition {name:?} exists in both the master and v2 schema bundles \
             with DIFFERENT content, and is not in EXPECTED_DIVERGENT_COLLISIONS. v2 always wins \
             collisions, so this would silently substitute v2's copy of {name:?} into any \
             master-derived type that references it - which may be wrong if the two copies now mean \
             different things. Either this is a legitimate new divergence (add {name:?} to \
             EXPECTED_DIVERGENT_COLLISIONS with a comment explaining why), or something has gone \
             wrong upstream in how codex generates these two bundles."
        );
    }
    Ok(())
}

/// Merges the master bundle's flat top-level definitions (ref-rewritten)
/// with the v2 bundle's definitions (v2 wins name collisions), applies the
/// `McpServerElicitationRequestParams` flatten workaround, and wraps the
/// result in the same schema envelope `build_combined_schema.py` produced.
pub fn build_combined(master: &Value, v2: &Value) -> Result<Value> {
    let master_defs = master
        .get("definitions")
        .and_then(Value::as_object)
        .context("master bundle missing top-level \"definitions\" object")?;
    let v2_defs = v2
        .get("definitions")
        .and_then(Value::as_object)
        .context("v2 bundle missing top-level \"definitions\" object")?;

    let mut master_flat_rewritten: Map<String, Value> = Map::new();
    for (k, v) in master_defs {
        if k == "v2" {
            continue;
        }
        master_flat_rewritten.insert(k.clone(), rewrite_v2_refs(v));
    }

    check_collision_compatibility(&master_flat_rewritten, v2_defs)?;

    let mut combined_defs = master_flat_rewritten;
    for (k, v) in v2_defs {
        // v2 wins name collisions - `Map::insert` on an existing key updates
        // its value, matching Python's `{**master_flat_rewritten,
        // **v2["definitions"]}` merge semantics. Iteration order of the
        // result is NOT meaningful (see xtask/Cargo.toml's serde_json entry -
        // `preserve_order` was removed after it leaked into unrelated
        // generated docs), only which value wins per key.
        combined_defs.insert(k.clone(), v.clone());
    }

    if let Some(schema) = combined_defs
        .get("McpServerElicitationRequestParams")
        .cloned()
    {
        combined_defs.insert(
            "McpServerElicitationRequestParams".to_string(),
            flatten_base_plus_oneof(&schema)?,
        );
    }

    Ok(wrap_definitions(combined_defs))
}

/// Wraps a flat `definitions` map in the schema envelope the crate's
/// `build.rs` expects (`$schema`/`title`/`description`/`type`/`definitions`,
/// in that field order).
pub fn wrap_definitions(definitions: Map<String, Value>) -> Value {
    let mut out = Map::new();
    out.insert(
        "$schema".to_string(),
        json!("http://json-schema.org/draft-07/schema#"),
    );
    out.insert("title".to_string(), json!("CodexAppServerProtocolCombined"));
    out.insert(
        "description".to_string(),
        json!(
            "Merged, flat-ref, self-contained v2-only Codex app-server protocol schema \
             (master envelope/ServerRequest/ClientNotification types ref-rewritten to flat \
             + v2 client-request/notification surface). Generated by \
             `cargo xtask codex-schema regen`."
        ),
    );
    out.insert("type".to_string(), json!("object"));
    out.insert("definitions".to_string(), Value::Object(definitions));
    Value::Object(out)
}

/// Extracts the `method` string of every `oneOf` branch of a discriminated
/// union definition (e.g. `ClientRequest`), in schema order.
pub fn methods_of(union_def: &Value) -> Result<Vec<String>> {
    let one_of = union_def
        .get("oneOf")
        .and_then(Value::as_array)
        .context("union definition missing \"oneOf\" array")?;
    one_of
        .iter()
        .map(|entry| {
            entry
                .pointer("/properties/method/enum/0")
                .and_then(Value::as_str)
                .map(str::to_string)
                .context("oneOf entry missing properties.method.enum[0]")
        })
        .collect()
}

/// The resolved params type for one method's `oneOf` branch: the referenced
/// type name (if any) and whether the params were nullable (2-way anyOf).
#[derive(Debug)]
pub struct ParamsType {
    pub type_name: Option<String>,
    pub optional: bool,
}

/// STRICT: only a plain `$ref`, a 2-way nullable-ref `anyOf`, an explicit
/// `{"type": "null"}`, or a missing `"params"` key are recognized. Any other
/// shape is a hard failure, matching the Python original's
/// `params_type_for` docstring: raising here (rather than silently falling
/// back to "no params") is load-bearing - the wrapper codegen in `build.rs`
/// would otherwise silently emit `params: ()` for a method that actually
/// requires typed params.
pub fn params_type_for(method: &str, union_def: &Value) -> Result<ParamsType> {
    let one_of = union_def
        .get("oneOf")
        .and_then(Value::as_array)
        .context("union definition missing \"oneOf\" array")?;

    for entry in one_of {
        let entry_method = entry
            .pointer("/properties/method/enum/0")
            .and_then(Value::as_str);
        if entry_method != Some(method) {
            continue;
        }

        let Some(params_schema) = entry.pointer("/properties/params") else {
            return Ok(ParamsType {
                type_name: None,
                optional: false,
            });
        };
        if params_schema.get("type").and_then(Value::as_str) == Some("null") {
            return Ok(ParamsType {
                type_name: None,
                optional: false,
            });
        }
        if let Some(r) = params_schema.get("$ref").and_then(Value::as_str) {
            return Ok(ParamsType {
                type_name: Some(ref_name(r)),
                optional: false,
            });
        }
        if let Some(any_of) = params_schema.get("anyOf").and_then(Value::as_array) {
            if any_of.len() == 2 {
                let refs: Vec<&Value> = any_of.iter().filter(|b| b.get("$ref").is_some()).collect();
                let nulls: Vec<&Value> = any_of
                    .iter()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("null"))
                    .collect();
                if refs.len() == 1 && nulls.len() == 1 {
                    let r = refs[0].get("$ref").and_then(Value::as_str).unwrap();
                    return Ok(ParamsType {
                        type_name: Some(ref_name(r)),
                        optional: true,
                    });
                }
            }
        }
        bail!(
            "{method}: unrecognized 'params' schema shape {params_schema} - not a plain $ref, a \
             nullable $ref, or an explicit null. Update params_type_for to handle this shape (or \
             the wrapper codegen in build.rs will silently emit `params: ()` for a method that \
             actually requires typed params)."
        );
    }
    bail!("{method}: not found in the given union's oneOf branches");
}

fn ref_name(r: &str) -> String {
    r.rsplit('/').next().unwrap_or(r).to_string()
}

/// One `client_requests`/`server_requests` manifest entry (includes a
/// resolved response type). Field order is JSON serialization order - must
/// stay `method, variant_name, fn_name, params_type, params_optional,
/// response_type` to byte-match the Python original's output.
#[derive(serde::Serialize)]
pub struct RequestEntry {
    pub method: String,
    pub variant_name: String,
    pub fn_name: String,
    pub params_type: Option<String>,
    pub params_optional: bool,
    pub response_type: Option<String>,
}

/// One `server_notifications`/`client_notifications` manifest entry - no
/// `response_type` field at all (not even `null`), matching the Python
/// original's dict literal for notifications.
#[derive(serde::Serialize)]
pub struct NotificationEntry {
    pub method: String,
    pub variant_name: String,
    pub fn_name: String,
    pub params_type: Option<String>,
    pub params_optional: bool,
}

#[derive(serde::Serialize)]
pub struct MethodsManifest {
    pub client_requests: Vec<RequestEntry>,
    pub server_requests: Vec<RequestEntry>,
    pub server_notifications: Vec<NotificationEntry>,
    pub client_notifications: Vec<NotificationEntry>,
}

/// Builds the full `methods.json` manifest from a merged, flat `definitions`
/// map (as produced by `build_combined`).
pub fn build_methods_manifest(combined_defs: &Map<String, Value>) -> Result<MethodsManifest> {
    let client_request_union = get_def(combined_defs, "ClientRequest")?;
    let server_request_union = get_def(combined_defs, "ServerRequest")?;
    let server_notification_union = get_def(combined_defs, "ServerNotification")?;
    let client_notification_union = get_def(combined_defs, "ClientNotification")?;

    let mut client_requests = Vec::new();
    for m in methods_of(client_request_union)? {
        let p = params_type_for(&m, client_request_union)?;
        let response_type = naming::resolve_response(&m, combined_defs)?;
        client_requests.push(RequestEntry {
            variant_name: naming::method_to_pascal(&m),
            fn_name: naming::method_to_snake_fn(&m),
            params_type: p.type_name,
            params_optional: p.optional,
            response_type,
            method: m,
        });
    }

    let mut server_requests = Vec::new();
    for m in methods_of(server_request_union)? {
        let p = params_type_for(&m, server_request_union)?;
        let response_type = naming::resolve_response(&m, combined_defs)?;
        server_requests.push(RequestEntry {
            variant_name: naming::method_to_pascal(&m),
            fn_name: naming::method_to_snake_fn(&m),
            params_type: p.type_name,
            params_optional: p.optional,
            response_type,
            method: m,
        });
    }

    let mut server_notifications = Vec::new();
    for m in methods_of(server_notification_union)? {
        let p = params_type_for(&m, server_notification_union)?;
        server_notifications.push(NotificationEntry {
            variant_name: naming::method_to_pascal(&m),
            fn_name: naming::method_to_snake_fn(&m),
            params_type: p.type_name,
            params_optional: p.optional,
            method: m,
        });
    }

    let mut client_notifications = Vec::new();
    for m in methods_of(client_notification_union)? {
        let p = params_type_for(&m, client_notification_union)?;
        client_notifications.push(NotificationEntry {
            variant_name: naming::method_to_pascal(&m),
            fn_name: naming::method_to_snake_fn(&m),
            params_type: p.type_name,
            params_optional: p.optional,
            method: m,
        });
    }

    Ok(MethodsManifest {
        client_requests,
        server_requests,
        server_notifications,
        client_notifications,
    })
}

fn get_def<'a>(defs: &'a Map<String, Value>, name: &str) -> Result<&'a Value> {
    defs.get(name)
        .with_context(|| format!("combined definitions missing required union type {name:?}"))
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
