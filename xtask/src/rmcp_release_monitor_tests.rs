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
    let payload = format!("{}\u{1f600}tail", "x".repeat(super::JSON_PREVIEW_BYTES - 2));

    let head = super::escaped_head(&payload, super::JSON_PREVIEW_BYTES);

    assert!(head.starts_with('"'));
    assert!(
        head.ends_with("\"..."),
        "truncated preview must be marked: {head}"
    );
    assert!(head.is_ascii(), "preview must be pure ASCII: {head}");
}

#[test]
fn well_formed_payloads_are_not_misdiagnosed() {
    assert!(super::diagnose_json_payload("[{\"tag_name\":\"rmcp-v1.8.0\"}]").is_none());
    assert!(super::diagnose_json_payload(VALID_CRATE_JSON).is_none());
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
    // The fetch steps set it per step, so precedence over GITHUB_ENV is not in
    // question; count them so a newly added gh step is not silently missed.
    assert_eq!(
        workflow.matches("CLICOLOR_FORCE: \"0\"").count(),
        5,
        "every step that runs gh must disable forced color"
    );
}
