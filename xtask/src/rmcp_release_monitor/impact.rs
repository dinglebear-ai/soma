//! Static "does this repo reference the changed upstream terms" scan, shared by
//! the MCP schema and conformance watches.
//!
//! Both watches answer the same question after detecting drift: which files in
//! *this* repo mention identifiers that moved upstream. The identifier
//! extraction, the stop list that keeps the shortlist signal-bearing, the
//! filesystem walk, and the Markdown table that renders the result are one
//! concern with two callers, so they live here rather than in either watch.

use anyhow::Result;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug)]
pub(super) struct RepoImpact {
    path: String,
    identifiers: Vec<String>,
}

pub(super) fn append_impact_section(body: &mut String, title: &str, impacts: &[RepoImpact]) {
    body.push_str(&format!("### {title}\n\n"));
    body.push_str("_Static identifier matches from upstream changes. Treat this as an inspection shortlist, not a complete migration plan._\n\n");
    if impacts.is_empty() {
        body.push_str("No direct local references to changed upstream terms were found.\n\n");
        return;
    }
    body.push_str("| Local file | Changed upstream terms referenced |\n");
    body.push_str("|---|---|\n");
    for impact in impacts.iter().take(25) {
        let terms = impact
            .identifiers
            .iter()
            .take(8)
            .map(|term| format!("`{term}`"))
            .collect::<Vec<_>>()
            .join(", ");
        body.push_str(&format!("| `{}` | {} |\n", impact.path, terms));
    }
    if impacts.len() > 25 {
        body.push_str(&format!("| _{} more files_ |  |\n", impacts.len() - 25));
    }
    body.push('\n');
}

pub(super) fn collect_identifiers(text: &str, terms: &mut BTreeSet<String>) {
    let mut current = String::new();
    for ch in text.chars() {
        if ch == '_' || ch == '-' || ch.is_ascii_alphanumeric() {
            current.push(ch);
        } else {
            push_identifier(&current, terms);
            current.clear();
        }
    }
    push_identifier(&current, terms);
}

fn push_identifier(identifier: &str, terms: &mut BTreeSet<String>) {
    let trimmed = identifier.trim_matches(|ch: char| ch == '_' || ch == '-');
    if trimmed.len() < 3 || trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return;
    }
    let normalized = trimmed.replace('-', "_");
    if is_stop_identifier(&normalized) {
        return;
    }
    terms.insert(normalized);
}

fn is_stop_identifier(identifier: &str) -> bool {
    matches!(
        identifier,
        "add"
            | "all"
            | "and"
            | "any"
            | "api"
            | "are"
            | "arr"
            | "auth"
            | "body"
            | "bool"
            | "const"
            | "default"
            | "derive"
            | "else"
            | "enum"
            | "export"
            | "false"
            | "for"
            | "from"
            | "get"
            | "impl"
            | "interface"
            | "let"
            | "main"
            | "mod"
            | "new"
            | "not"
            | "null"
            | "number"
            | "object"
            | "one"
            | "option"
            | "pub"
            | "ref"
            | "self"
            | "serde"
            | "some"
            | "string"
            | "test"
            | "this"
            | "true"
            | "type"
            | "undefined"
            | "use"
            | "vec"
            | "with"
    )
}

pub(super) fn scan_repo_impacts(root: &Path, terms: &BTreeSet<String>) -> Result<Vec<RepoImpact>> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut impacts = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).into_iter() {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type().is_dir() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if !is_repo_scan_file(path) || is_skipped_repo_path(&relative) {
            continue;
        }
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let matched = terms
            .iter()
            .filter(|term| text.contains(term.as_str()))
            .take(12)
            .cloned()
            .collect::<Vec<_>>();
        if matched.is_empty() {
            continue;
        }
        impacts.push(RepoImpact {
            path: relative,
            identifiers: matched,
        });
        if impacts.len() >= 40 {
            break;
        }
    }
    impacts.sort_by(|left, right| {
        right
            .identifiers
            .len()
            .cmp(&left.identifiers.len())
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(impacts)
}

fn is_repo_scan_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("rs" | "toml" | "json" | "yaml" | "yml" | "md" | "mdx" | "ts" | "tsx" | "js" | "jsx")
    )
}

fn is_skipped_repo_path(text: &str) -> bool {
    [
        ".git/",
        "target/",
        "node_modules/",
        "dist/",
        ".next/",
        "docs/references/mcp/schema/",
    ]
    .iter()
    .any(|needle| {
        text == needle.trim_end_matches('/')
            || text.starts_with(needle)
            || text.contains(&format!("/{needle}"))
    }) || text == "Cargo.lock"
        || text.ends_with("/Cargo.lock")
}
