use anyhow::{Context, Result};
use std::{fs, path::Path};

pub(super) fn read_file(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

pub(super) fn display_path(path: &Path) -> String {
    path.strip_prefix(".")
        .unwrap_or(path)
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string()
}

pub(super) fn contains_top_level_json_key(text: &str, key: &str) -> bool {
    // Avoids serde_json in xtask. Handles both formatted JSON (key on its own
    // line) and compact/single-line JSON (key after `{`).
    let pattern = format!("\"{key}\"");
    text.lines().any(|line| {
        let content = line.trim_start().trim_start_matches('{').trim_start();
        content.starts_with(&pattern) && content[pattern.len()..].trim_start().starts_with(':')
    })
}

pub(super) fn size_limit(path: &Path) -> Option<usize> {
    if path == Path::new("crates/shared/auth/src/sqlite.rs") {
        // Vendored auth storage logic is larger than Soma surface modules;
        // keep it visible as a warning without blocking unrelated CI gates.
        return Some(700);
    }
    if path == Path::new("crates/soma/application/src/provider_registry.rs") {
        // Provider registration and dispatch is intentionally centralized while
        // the drop-in provider contract is settling. Keep it warning-visible.
        return Some(400);
    }
    if path == Path::new("xtask/src/generated_surfaces.rs") {
        // Generated surface docs, plugin metadata, and package catalogs are
        // coupled by design; split once the generated contract stabilizes.
        return Some(500);
    }
    if path == Path::new("xtask/src/rmcp_release_monitor.rs")
        || path == Path::new("xtask/src/scaffold.rs")
    {
        // These xtask modules orchestrate cross-cutting repo automation and are
        // being split as the automation surface settles. Keep them visible as
        // warnings without making unrelated workflow fixes fail CI.
        return Some(600);
    }
    if path
        .to_string_lossy()
        .starts_with("xtask/src/scripts_lane_")
    {
        // Transitional script ports are grouped by migration lane. Keep them
        // visible as warnings while avoiding a hard block during the shell to
        // xtask handoff.
        return Some(1000);
    }

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("rs") => Some(350),
        Some("ts" | "tsx") => Some(300),
        _ => None,
    }
}

pub(super) fn is_size_exempt(path: &Path) -> bool {
    let path = path.to_string_lossy();
    // Vendored upstream MCP schema mirrors.
    if path.starts_with("docs/references/mcp/schema/") {
        return true;
    }
    // Checked-in code generator output. `docs/PATTERNS.md` already states the
    // policy ("must split unless generated/fixture/schema mirror"); this is
    // the `generated` half of it. Splitting is not an option a maintainer has
    // here - the file is rewritten wholesale by its generator on every run,
    // and a parity test fails the build if it is hand-edited - so a size
    // warning on one would be pure noise, never actionable.
    //
    // Deliberately narrow: only a `src/generated/` directory counts, not a
    // file that merely says "generated" somewhere in its path, so this can't
    // be used to launder a hand-written module past the limit.
    if path.contains("/src/generated/") {
        return true;
    }
    false
}

pub(super) fn is_test_file(path: &Path) -> bool {
    let path = path.to_string_lossy();
    path.contains("/tests/")
        || path.ends_with("_test.rs")
        || path.ends_with("/tests.rs")
        || path.ends_with(".test.ts")
        || path.ends_with(".test.tsx")
        || path.ends_with(".spec.ts")
        || path.ends_with(".spec.tsx")
        || path.contains("/__tests__/")
}

pub(super) fn effective_loc(path: &Path) -> Result<usize> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(effective_loc_from_text(
        &text,
        path.extension().and_then(|ext| ext.to_str()) == Some("rs"),
    ))
}

fn effective_loc_from_text(text: &str, strip_tests: bool) -> usize {
    let text = if strip_tests {
        strip_inline_test_module(text)
    } else {
        text
    };
    let mut count = 0usize;
    let mut in_block = false;

    for raw in text.lines() {
        let mut line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if in_block {
            if let Some((_, after)) = line.split_once("*/") {
                line = after.trim();
                in_block = false;
                if line.is_empty() {
                    continue;
                }
            } else {
                continue;
            }
        }
        if line.starts_with("//") {
            continue;
        }
        if line.starts_with("/*") {
            if let Some((_, after)) = line.split_once("*/") {
                line = after.trim();
                if line.is_empty() {
                    continue;
                }
            } else {
                in_block = true;
                continue;
            }
        }
        count += 1;
    }
    count
}

fn strip_inline_test_module(text: &str) -> &str {
    let lines = text.lines().collect::<Vec<_>>();
    for index in 0..lines.len().saturating_sub(1) {
        if lines[index].trim() != "#[cfg(test)]" {
            continue;
        }
        let next = lines[index + 1].trim_start();
        if next.starts_with("mod ") || next.starts_with("pub mod ") {
            let byte_index = lines[..index].iter().map(|line| line.len() + 1).sum();
            return &text[..byte_index];
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_loc_ignores_comments_blanks_and_trailing_tests() {
        let text = r#"
// comment
pub fn production() {}

/* block
   comment */
pub fn more() {}

#[cfg(test)]
mod tests {
    fn test_only() {}
}
"#;
        assert_eq!(effective_loc_from_text(text, true), 2);
    }

    #[test]
    fn effective_loc_counts_code_after_inline_block_comment() {
        let text = "/* license */ pub fn one() {}\nlet two = 2;";
        assert_eq!(effective_loc_from_text(text, false), 2);
    }

    #[test]
    fn top_level_json_key_detects_manifest_version_field() {
        assert!(contains_top_level_json_key(
            "{\n  \"version\": \"1\"\n}",
            "version"
        ));
        assert!(!contains_top_level_json_key(
            "{\n  \"not_version\": true\n}",
            "version"
        ));
    }

    #[test]
    fn top_level_json_key_handles_compact_json() {
        // Single-line / compact JSON: key appears after `{`
        assert!(contains_top_level_json_key(
            "{ \"version\": \"1\" }",
            "version"
        ));
        assert!(contains_top_level_json_key(
            "{\"version\":\"1\"}",
            "version"
        ));
        // Must not match a different key
        assert!(!contains_top_level_json_key(
            "{ \"name\": \"foo\" }",
            "version"
        ));
    }

    #[test]
    fn size_limits_skip_vendored_mcp_schema_references() {
        assert!(is_size_exempt(Path::new(
            "docs/references/mcp/schema/2025-11-25/schema.ts"
        )));
        assert!(!is_size_exempt(Path::new("apps/web/src/app/page.tsx")));
    }

    #[test]
    fn size_limits_skip_checked_in_generator_output() {
        assert!(is_size_exempt(Path::new(
            "crates/shared/codex-app-server-client/clients/typescript/src/generated/openapi-types.ts"
        )));
        // Only a real `src/generated/` directory is exempt - a hand-written
        // module can't opt out by working the word into its name or docs.
        assert!(!is_size_exempt(Path::new(
            "crates/soma/mcp/src/generated_schemas.rs"
        )));
        assert!(!is_size_exempt(Path::new(
            "xtask/src/generated_surfaces.rs"
        )));
    }

    #[test]
    fn transitional_xtask_modules_warn_before_hard_failing() {
        assert_eq!(
            size_limit(Path::new("xtask/src/rmcp_release_monitor.rs")),
            Some(600)
        );
        assert_eq!(size_limit(Path::new("xtask/src/scaffold.rs")), Some(600));
    }
}
