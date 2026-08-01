#[test]
fn exact_version_strips_cargo_requirement_comparators() {
    // The workspace pins rmcp exactly; the monitor must still parse it.
    assert_eq!(super::exact_version("=3.0.0-beta.2"), "3.0.0-beta.2");
    assert_eq!(super::exact_version("=2.2.0"), "2.2.0");
    assert_eq!(super::exact_version("^1.2.3"), "1.2.3");
    assert_eq!(super::exact_version("~1.2.3"), "1.2.3");
    assert_eq!(super::exact_version(" 1.2.3 "), "1.2.3");
    assert!(semver::Version::parse(super::exact_version("=3.0.0-beta.2")).is_ok());
}

#[test]
fn conformance_defaults_match_the_workspace_rmcp_pin() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let requirement = super::detect_current_rmcp_version(&root).expect("workspace rmcp pin");
    let version = super::exact_version(&requirement);
    let baseline: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("conformance/upstream-baseline.json"))
            .expect("conformance baseline"),
    )
    .expect("valid conformance baseline JSON");
    let commit = baseline["rmcp"]["commit"]
        .as_str()
        .expect("baseline rmcp commit");
    assert_eq!(baseline["rmcp"]["crate_version"], version);
    assert_eq!(baseline["rmcp"]["release_tag"], format!("rmcp-v{version}"));

    let script = std::fs::read_to_string(root.join("scripts/ci/mcp-conformance.sh"))
        .expect("conformance script");
    assert!(
        script.contains(&format!("RMCP_VERSION=\"${{RMCP_VERSION:-{version}}}\"")),
        "conformance script version must match workspace pin {version}"
    );
    assert!(
        script.contains(&format!("RMCP_COMMIT=\"${{RMCP_COMMIT:-{commit}}}\"")),
        "conformance script commit must match upstream baseline {commit}"
    );
}

#[test]
fn conformance_script_reserves_parallel_safe_ports() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let script = std::fs::read_to_string(root.join("scripts/ci/mcp-conformance.sh"))
        .expect("conformance script");
    assert!(
        script.contains("PORT=\"${MCP_CONFORMANCE_PORT:-}\""),
        "the default port must be dynamically allocated"
    );
    assert!(
        !script.contains("MCP_CONFORMANCE_PORT:-18002"),
        "parallel jobs must not share a fixed default port"
    );
    assert!(script.contains("soma-mcp-conformance-port-${candidate}.lock"));
    assert!(script.contains("mkdir \"$candidate_lock\""));
    assert!(script.contains("rmdir \"$PORT_LOCK\""));
    assert!(script.contains("/dev/tcp/127.0.0.1/${candidate}"));
    assert!(!script.contains("command -v ss"));
    assert!(script.contains("MCP_CONFORMANCE_UPSTREAM_TARGET_DIR"));
    assert!(script.contains("CLIENT=\"$UPSTREAM_TARGET/debug/conformance-client\""));
}

/// Minimal valid crates.io payload; these tests only care about the releases
/// argument, which `build_monitor_report` parses second.
const VALID_CRATE_JSON: &str = r#"{
  "crate": {"name": "rmcp", "max_version": "1.8.0"},
  "versions": [{"num": "1.8.0", "created_at": "2026-06-23T12:28:57Z", "yanked": false}]
}"#;

fn releases_error(releases_json: &str) -> String {
    let error =
        super::build_monitor_report("1.7.0", VALID_CRATE_JSON, releases_json, None, None, 60_000)
            .expect_err("malformed releases payload must not parse");
    format!("{error:#}")
}

/// The production failure. `zackees/setup-soldr` exports `CLICOLOR_FORCE=1`
/// into `GITHUB_ENV`, every later step inherits it, and `gh api` then writes
/// pretty-printed *colorized* JSON even with stdout redirected to a file. The
/// payload therefore begins with an ESC byte and serde reports only "expected
/// value at line 1 column 1", which identifies nothing.
#[test]
fn ansi_colorized_releases_payload_reports_the_escape_and_the_fix() {
    let colorized = "\u{1b}[1;37m[\u{1b}[m\n  \u{1b}[1;37m{\u{1b}[m\n    \u{1b}[1;34m\"tag_name\"\u{1b}[m\u{1b}[1;38;5;245m:\u{1b}[m \u{1b}[32m\"rmcp-v1.8.0\"\u{1b}[m\n";

    let message = releases_error(colorized);

    assert!(
        message.contains("GitHub release metadata (--releases-json)"),
        "error must name the offending input: {message}"
    );
    assert!(
        message.contains("ANSI escape"),
        "error must diagnose the colorized payload: {message}"
    );
    assert!(
        message.contains("CLICOLOR_FORCE=0"),
        "error must name the remediation: {message}"
    );
    assert!(
        message.contains(&format!("{} bytes", colorized.len())),
        "error must report the payload length: {message}"
    );
    assert!(
        message.contains("\\u{1b}[1;37m["),
        "error must show an escaped head of the payload: {message}"
    );
    assert!(
        !message.contains('\u{1b}'),
        "the preview must be escaped, not raw ESC bytes: {message}"
    );
}

/// A GitHub API error object parses as JSON but not as `Vec<GithubRelease>`.
/// Serde calls it "invalid type: map, expected a sequence", which never
/// mentions auth, permissions, or rate limits.
#[test]
fn github_error_object_releases_payload_is_identified_as_an_api_error() {
    let message = releases_error(
        r#"{"message":"API rate limit exceeded","documentation_url":"https://docs.github.com/rest"}"#,
    );

    assert!(
        message.contains("GitHub API error object"),
        "error must identify the API error shape: {message}"
    );
    assert!(
        message.contains("rate limit"),
        "error must point at the likely causes: {message}"
    );
    assert!(
        message.contains("API rate limit exceeded"),
        "error must echo the upstream message: {message}"
    );
}

#[test]
fn empty_releases_payload_says_the_fetch_wrote_nothing() {
    let message = releases_error("");

    assert!(
        message.contains("payload is empty"),
        "error must call out the empty payload: {message}"
    );
    assert!(
        message.contains("0 bytes"),
        "error must report the payload length: {message}"
    );
    assert!(
        message.contains("GitHub release metadata (--releases-json)"),
        "error must name the offending input: {message}"
    );
}

#[test]
fn byte_order_mark_payload_is_identified_as_a_bom() {
    // Every other branch of diagnose_json_payload has a regression test; this
    // one covers the BOM branch so the diagnosis list stays evidence-backed
    // rather than an untested grab bag.
    let message = releases_error("\u{feff}[{\"tag_name\":\"rmcp-v3.0.0-beta.2\"}]");

    assert!(
        message.contains("byte-order mark"),
        "error must identify a UTF-8 BOM: {message}"
    );
}

#[test]
fn html_error_page_payload_is_identified_as_html() {
    let message = releases_error("<!DOCTYPE html>\n<html><body>502 Bad Gateway</body></html>\n");

    assert!(
        message.contains("looks like HTML"),
        "error must identify an HTML error page: {message}"
    );
}

#[test]
fn payload_preview_is_truncated_on_a_char_boundary() {
    // A multi-byte char straddling the preview limit must not panic or emit
    // invalid UTF-8; the head is escaped to ASCII either way.
    let payload = format!(
        "{}\u{1f600}tail",
        "x".repeat(super::diagnostics::JSON_PREVIEW_BYTES - 2)
    );

    let head = super::diagnostics::escaped_head(&payload, super::diagnostics::JSON_PREVIEW_BYTES);

    assert!(head.starts_with('"'));
    assert!(
        head.ends_with("\"..."),
        "truncated preview must be marked: {head}"
    );
    assert!(head.is_ascii(), "preview must be pure ASCII: {head}");
}

#[test]
fn well_formed_payloads_are_not_misdiagnosed() {
    assert!(
        super::diagnostics::diagnose_json_payload("[{\"tag_name\":\"rmcp-v1.8.0\"}]").is_none()
    );
    assert!(super::diagnostics::diagnose_json_payload(VALID_CRATE_JSON).is_none());
}

/// The workflow half of the fix: the fetch steps must neutralize the
/// runner-level `CLICOLOR_FORCE=1`, and a validation step must reject a
/// non-JSON payload before it reaches xtask.
#[test]
fn workflow_disables_forced_color_and_validates_fetched_payloads() {
    let workflow = include_str!("../../.github/workflows/rmcp-release-monitor.yml");

    assert!(
        workflow.contains("CLICOLOR_FORCE: \"0\""),
        "gh steps must neutralize the runner-level CLICOLOR_FORCE=1"
    );
    assert!(
        workflow.contains("Validate fetched JSON payloads"),
        "fetched payloads must be validated before xtask parses them"
    );
    assert!(
        workflow.contains("rmcp-releases.json"),
        "the releases payload must be covered by validation"
    );
}

/// Derived guard: every workflow step whose `run:` block invokes `gh` must
/// disable forced color. Checked by walking the steps rather than counting
/// occurrences, so adding a legitimate `gh` step fails with the step's name
/// instead of an off-by-one on a magic number - and so both workflows that
/// use the soldr runner action are covered, not just one.
#[test]
fn every_gh_step_disables_forced_color() {
    for (name, source) in [
        (
            "rmcp-release-monitor.yml",
            include_str!("../../.github/workflows/rmcp-release-monitor.yml"),
        ),
        (
            "codex-schema-drift-monitor.yml",
            include_str!("../../.github/workflows/codex-schema-drift-monitor.yml"),
        ),
    ] {
        for step in source.split("      - name: ").skip(1) {
            let title = step.lines().next().unwrap_or_default().trim();
            // Skip YAML comments: the rationale comments in these workflows
            // mention `gh issue ...` in prose, which would otherwise match.
            let runs_gh = step
                .lines()
                .filter(|line| !line.trim_start().starts_with('#'))
                .any(|line| line.contains("gh api") || line.contains("gh issue"));
            if runs_gh {
                assert!(
                    step.contains("CLICOLOR_FORCE: \"0\""),
                    "{name}: step `{title}` runs gh but does not disable forced color"
                );
            }
        }
    }
}
