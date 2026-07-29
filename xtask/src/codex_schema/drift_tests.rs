use super::*;
use serde_json::json;

fn manifest(entries: Value) -> Value {
    json!({
        "client_requests": entries,
        "server_requests": [],
        "server_notifications": [],
        "client_notifications": [],
    })
}

fn request_entry(method: &str, params_type: &str, response_type: &str) -> Value {
    json!({
        "method": method,
        "variant_name": "Ignored",
        "fn_name": "ignored",
        "params_type": params_type,
        "params_optional": false,
        "response_type": response_type,
    })
}

// ---------------------------------------------------------------------------
// diff_section
// ---------------------------------------------------------------------------

#[test]
fn diff_section_detects_added_method() {
    let vendored = manifest(json!([request_entry("a/one", "AParams", "AResponse")]));
    let installed = manifest(json!([
        request_entry("a/one", "AParams", "AResponse"),
        request_entry("b/two", "BParams", "BResponse"),
    ]));

    let diff = diff_section("client_requests", &vendored, &installed).unwrap();

    assert!(!diff.in_sync());
    assert_eq!(diff.vendored_count, 1);
    assert_eq!(diff.installed_count, 2);
    assert_eq!(diff.added.len(), 1);
    assert_eq!(diff.added[0].method, "b/two");
    assert!(diff.removed.is_empty());
    assert!(diff.changed.is_empty());
}

#[test]
fn diff_section_detects_removed_method() {
    let vendored = manifest(json!([
        request_entry("a/one", "AParams", "AResponse"),
        request_entry("b/two", "BParams", "BResponse"),
    ]));
    let installed = manifest(json!([request_entry("a/one", "AParams", "AResponse")]));

    let diff = diff_section("client_requests", &vendored, &installed).unwrap();

    assert!(!diff.in_sync());
    assert_eq!(diff.removed.len(), 1);
    assert_eq!(diff.removed[0].method, "b/two");
    assert!(diff.added.is_empty());
    assert!(diff.changed.is_empty());
}

#[test]
fn diff_section_detects_changed_params_type() {
    let vendored = manifest(json!([request_entry("a/one", "OldParams", "AResponse")]));
    let installed = manifest(json!([request_entry("a/one", "NewParams", "AResponse")]));

    let diff = diff_section("client_requests", &vendored, &installed).unwrap();

    assert!(!diff.in_sync());
    assert_eq!(diff.changed.len(), 1);
    let changed = &diff.changed[0];
    assert_eq!(changed.method, "a/one");
    assert_eq!(changed.vendored.params_type.as_deref(), Some("OldParams"));
    assert_eq!(changed.installed.params_type.as_deref(), Some("NewParams"));
}

#[test]
fn diff_section_detects_changed_params_optional() {
    let mut vendored_entry = request_entry("a/one", "AParams", "AResponse");
    vendored_entry["params_optional"] = json!(false);
    let mut installed_entry = request_entry("a/one", "AParams", "AResponse");
    installed_entry["params_optional"] = json!(true);

    let vendored = manifest(json!([vendored_entry]));
    let installed = manifest(json!([installed_entry]));

    let diff = diff_section("client_requests", &vendored, &installed).unwrap();

    assert_eq!(diff.changed.len(), 1);
    assert!(!diff.changed[0].vendored.params_optional);
    assert!(diff.changed[0].installed.params_optional);
}

#[test]
fn diff_section_detects_changed_response_type() {
    let vendored = manifest(json!([request_entry("a/one", "AParams", "OldResponse")]));
    let installed = manifest(json!([request_entry("a/one", "AParams", "NewResponse")]));

    let diff = diff_section("client_requests", &vendored, &installed).unwrap();

    assert_eq!(diff.changed.len(), 1);
    assert_eq!(
        diff.changed[0].vendored.response_type.as_deref(),
        Some("OldResponse")
    );
    assert_eq!(
        diff.changed[0].installed.response_type.as_deref(),
        Some("NewResponse")
    );
}

#[test]
fn diff_section_in_sync_when_identical() {
    let entries = json!([
        request_entry("a/one", "AParams", "AResponse"),
        request_entry("b/two", "BParams", "BResponse"),
    ]);
    let vendored = manifest(entries.clone());
    let installed = manifest(entries);

    let diff = diff_section("client_requests", &vendored, &installed).unwrap();

    assert!(diff.in_sync());
    assert_eq!(diff.drifted_count(), 0);
    assert_eq!(diff.vendored_count, 2);
    assert_eq!(diff.installed_count, 2);
}

#[test]
fn diff_section_notification_entries_without_response_type_key_are_in_sync() {
    // Notification sections never carry a "response_type" key at all (see
    // merge::NotificationEntry) - both sides missing the key must compare as
    // equal, not as a false "changed" (None vs None).
    let entry = json!({
        "method": "turn/completed",
        "variant_name": "TurnCompleted",
        "fn_name": "turn_completed",
        "params_type": "TurnCompletedParams",
        "params_optional": false,
    });
    let vendored = json!({
        "client_requests": [],
        "server_requests": [],
        "server_notifications": [entry.clone()],
        "client_notifications": [],
    });
    let installed = json!({
        "client_requests": [],
        "server_requests": [],
        "server_notifications": [entry],
        "client_notifications": [],
    });

    let diff = diff_section("server_notifications", &vendored, &installed).unwrap();

    assert!(diff.in_sync());
}

#[test]
fn section_map_rejects_duplicate_method_names() {
    let manifest = manifest(json!([
        request_entry("a/one", "AParams", "AResponse"),
        request_entry("a/one", "AParams", "AResponse"),
    ]));

    let err = section_map(&manifest, "client_requests").unwrap_err();
    assert!(err.to_string().contains("duplicate method"));
}

#[test]
fn section_map_requires_method_field() {
    let manifest = manifest(json!([{
        "variant_name": "Ignored",
        "fn_name": "ignored",
        "params_type": Value::Null,
        "params_optional": false,
        "response_type": Value::Null,
    }]));

    let err = section_map(&manifest, "client_requests").unwrap_err();
    assert!(
        err.to_string()
            .contains("missing a string \"method\" field")
    );
}

// ---------------------------------------------------------------------------
// build_report
// ---------------------------------------------------------------------------

fn four_section_manifest(client_requests: Value) -> Value {
    json!({
        "client_requests": client_requests,
        "server_requests": [],
        "server_notifications": [],
        "client_notifications": [],
    })
}

#[test]
fn build_report_in_sync_true_when_zero_drift_across_all_sections() {
    let entries = json!([request_entry("a/one", "AParams", "AResponse")]);
    let vendored = four_section_manifest(entries.clone());
    let installed = four_section_manifest(entries);

    let report = build_report(
        &vendored,
        &installed,
        "codex-cli 0.1.0",
        Some("codex-cli 0.1.0"),
    )
    .unwrap();

    assert!(report.in_sync);
    assert_eq!(report.drifted_count, 0);
    assert_eq!(report.version.matches, Some(true));
    assert_eq!(report.sections.len(), 4);
}

#[test]
fn build_report_counts_drift_and_flags_version_mismatch() {
    let vendored = four_section_manifest(json!([request_entry("a/one", "AParams", "AResponse")]));
    let installed = four_section_manifest(json!([
        request_entry("a/one", "AParams", "AResponse"),
        request_entry("b/two", "BParams", "BResponse"),
    ]));

    let report = build_report(
        &vendored,
        &installed,
        "codex-cli 0.1.0",
        Some("codex-cli 0.2.0"),
    )
    .unwrap();

    assert!(!report.in_sync);
    assert_eq!(report.drifted_count, 1);
    assert_eq!(report.version.vendored, "codex-cli 0.1.0");
    assert_eq!(report.version.installed.as_deref(), Some("codex-cli 0.2.0"));
    assert_eq!(report.version.matches, Some(false));
}

#[test]
fn build_report_version_matches_is_none_when_installed_version_unknown() {
    let entries = json!([request_entry("a/one", "AParams", "AResponse")]);
    let vendored = four_section_manifest(entries.clone());
    let installed = four_section_manifest(entries);

    let report = build_report(&vendored, &installed, "codex-cli 0.1.0", None).unwrap();

    assert!(report.in_sync, "method surfaces are identical");
    assert_eq!(report.version.installed, None);
    assert_eq!(report.version.matches, None);
}

// ---------------------------------------------------------------------------
// parse_args
// ---------------------------------------------------------------------------

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parse_args_defaults_to_no_dir_no_json_no_strict() {
    let options = parse_args(&args(&[])).unwrap();
    assert!(options.dir.is_none());
    assert!(!options.json);
    assert!(!options.strict);
}

#[test]
fn parse_args_parses_dir_json_and_strict_together() {
    let options = parse_args(&args(&["--dir", "/tmp/dump", "--json", "--strict"])).unwrap();
    assert_eq!(options.dir, Some(PathBuf::from("/tmp/dump")));
    assert!(options.json);
    assert!(options.strict);
}

#[test]
fn parse_args_rejects_unknown_flag() {
    let err = parse_args(&args(&["--bogus"])).unwrap_err();
    assert!(err.to_string().contains("unexpected argument"));
}

#[test]
fn parse_args_dir_requires_a_value() {
    let err = parse_args(&args(&["--dir"])).unwrap_err();
    assert!(err.to_string().contains("--dir requires a value"));
}
