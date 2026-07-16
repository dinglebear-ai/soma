//! Markdown-file-as-MCP-prompt loading: synthesizes a single-prompt
//! `static-rust` manifest `Value` from a `.md` file. Split out of
//! `filesystem.rs` to stay under the module size hard limit — see
//! "Markdown Prompts" in `docs/PROVIDERS.md`.

use std::{fs, path::Path};

use serde_json::{json, Value};

use super::FileProviderLoadError;

/// Synthesizes a single-prompt `static-rust` manifest `Value` from a Markdown
/// file: the file stem becomes both the provider name and the prompt name,
/// the first `# Heading` becomes the description, and the full file body
/// becomes the prompt `template`. Provider names and MCP primitive (prompt)
/// names live in separate uniqueness namespaces
/// (`filesystem_uniqueness::DirectoryNamespace`), so reusing the same slug
/// for both is safe and keeps `soma providers list`'s reported `provider_id`
/// matching the resulting `prompts/get` name.
pub(super) fn load_markdown_catalog_value(path: &Path) -> Result<Value, FileProviderLoadError> {
    let text = fs::read_to_string(path).map_err(|source| FileProviderLoadError {
        path: path.to_path_buf(),
        message: format!("failed to read Markdown provider: {source}"),
    })?;
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("prompt");
    let name = prompt_name_from_file_stem(stem);
    let description =
        first_markdown_heading(&text).unwrap_or_else(|| format!("Markdown prompt from {stem}"));

    Ok(json!({
        "schema_version": 1,
        "provider": {
            "name": name.clone(),
            "kind": "static-rust",
            "title": description,
            "description": format!("Markdown prompt provider loaded from {}", path.display()),
            "source": path.display().to_string(),
        },
        "prompts": [{
            "name": name,
            "description": description,
            "template": text,
        }],
    }))
}

/// Derives a schema-valid `name` (`^[a-z][a-z0-9]*(?:[-_][a-z0-9]+)*$`) from a
/// file stem: lowercases, collapses runs of non-alphanumerics to a single
/// hyphen, and trims trailing hyphens. Falls back to a `prompt-` prefix when
/// the result would not otherwise start with a lowercase letter.
fn prompt_name_from_file_stem(stem: &str) -> String {
    let mut output = String::new();
    let mut previous_separator = false;
    for ch in stem.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            output.push(ch);
            previous_separator = false;
        } else if !previous_separator && !output.is_empty() {
            output.push('-');
            previous_separator = true;
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    if output.is_empty() {
        return "prompt".to_owned();
    }
    if output
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase())
    {
        output
    } else {
        format!("prompt-{output}")
    }
}

fn first_markdown_heading(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let trimmed = line.trim();
        let heading = trimmed.strip_prefix("# ")?;
        let heading = heading.trim();
        (!heading.is_empty()).then(|| heading.to_owned())
    })
}

#[cfg(test)]
#[path = "filesystem_prompts_tests.rs"]
mod tests;
