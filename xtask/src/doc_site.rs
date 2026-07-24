//! Landing page and OpenAPI (Redoc) assets for `cargo xtask doc`.
//!
//! `cargo doc` leaves `target/doc/` as a bare pile of per-crate directories,
//! which is exactly what GitHub Pages would serve as the site root. This
//! module gives the doc root a real entry point:
//!
//! - `index.html` — a small landing page (inline CSS, no external assets)
//!   listing every workspace crate with its `description` from
//!   `cargo metadata`, each linking into that crate's rustdoc.
//! - `openapi.json` — a copy of the checked-in REST contract
//!   (`docs/generated/openapi.json`) so the doc site is self-contained.
//! - `openapi.html` — renders that contract with the Redoc standalone bundle
//!   (the one external asset, loaded from the Redoc CDN at view time).
//!
//! `.github/workflows/docs.yml` builds docs through `cargo xtask doc --strict`,
//! so what Pages deploys is exactly what this module writes — the workflow no
//! longer carries its own inline HTML step.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

/// Relative path (from the repo root) of the generated OpenAPI contract that
/// gets copied beside the landing page. Kept current by `cargo xtask
/// check-openapi`, so the doc site always renders the same contract CI gates.
const OPENAPI_SOURCE: &str = "docs/generated/openapi.json";

/// One workspace crate as rendered on the landing page.
struct CrateEntry {
    /// Package name as written in `Cargo.toml` (hyphenated).
    name: String,
    /// rustdoc output directory under `target/doc/` — the documented target's
    /// name with hyphens folded to underscores, which is how rustdoc names it.
    doc_dir: String,
    /// `description` from the crate manifest; empty when the crate has none.
    description: String,
}

/// Write the landing page and OpenAPI assets into `doc_root`
/// (normally `target/doc/`).
///
/// Called by `cargo xtask doc` after a successful `cargo doc` run — including
/// `-p` partial builds, where crate links into undocumented crates will 404
/// locally until a full workspace build fills them in. Emitting
/// unconditionally keeps the output deterministic: the page is derived from
/// `cargo metadata`, not from whichever subset happened to be built.
pub(crate) fn emit(doc_root: &Path) -> Result<()> {
    let entries = workspace_crates()?;
    std::fs::create_dir_all(doc_root)
        .with_context(|| format!("failed to create {}", doc_root.display()))?;

    let index_path = doc_root.join("index.html");
    std::fs::write(&index_path, landing_html(&entries))
        .with_context(|| format!("failed to write {}", index_path.display()))?;
    println!("==> Wrote landing page {}", index_path.display());

    let openapi_source = Path::new(OPENAPI_SOURCE);
    if openapi_source.is_file() {
        let spec_path = doc_root.join("openapi.json");
        std::fs::copy(openapi_source, &spec_path)
            .with_context(|| format!("failed to copy {OPENAPI_SOURCE} to doc root"))?;
        let redoc_path = doc_root.join("openapi.html");
        std::fs::write(&redoc_path, REDOC_HTML)
            .with_context(|| format!("failed to write {}", redoc_path.display()))?;
        println!("==> Wrote OpenAPI page {}", redoc_path.display());
    } else {
        // Tolerated rather than fatal so `cargo xtask doc` still works from a
        // stripped checkout; in this repo the contract is always tracked.
        println!("==> Skipped OpenAPI page: {OPENAPI_SOURCE} not found");
    }
    Ok(())
}

/// Enumerate workspace crates via `cargo metadata --no-deps`.
///
/// Parsed with `serde_json` rather than the `cargo_metadata` crate to keep
/// xtask dependency-light (see xtask/Cargo.toml's preamble). Only the fields
/// the landing page needs are read: package name, description, and the
/// documented target's name for the rustdoc directory.
fn workspace_crates() -> Result<Vec<CrateEntry>> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .stdin(Stdio::null())
        .output()
        .context("Failed to spawn `cargo metadata`")?;
    if !output.status.success() {
        bail!("`cargo metadata` exited with status {}", output.status);
    }
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("`cargo metadata` emitted invalid JSON")?;
    let packages = metadata
        .get("packages")
        .and_then(|value| value.as_array())
        .context("`cargo metadata` output has no `packages` array")?;

    let mut entries = Vec::new();
    for package in packages {
        let Some(name) = package.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(doc_target) = documented_target_name(package) else {
            // Nothing rustdoc would document (no lib/bin target) — skip.
            continue;
        };
        let description = package
            .get("description")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_owned();
        entries.push(CrateEntry {
            name: name.to_owned(),
            doc_dir: doc_target.replace('-', "_"),
            description,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// Pick the target `cargo doc` documents for a package: the lib-like target
/// when present (lib/rlib/dylib/cdylib/staticlib/proc-macro), otherwise the
/// first bin. When a package has both a lib and a same-named bin (apps/soma),
/// rustdoc documents the lib, so lib-first matches the output on disk.
fn documented_target_name(package: &serde_json::Value) -> Option<String> {
    let targets = package.get("targets")?.as_array()?;
    let kind_of = |target: &serde_json::Value| -> Vec<String> {
        target
            .get("kind")
            .and_then(|value| value.as_array())
            .map(|kinds| {
                kinds
                    .iter()
                    .filter_map(|kind| kind.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    const LIB_KINDS: &[&str] = &["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"];
    for target in targets {
        let kinds = kind_of(target);
        if kinds.iter().any(|kind| LIB_KINDS.contains(&kind.as_str())) {
            return target
                .get("name")
                .and_then(|value| value.as_str())
                .map(str::to_owned);
        }
    }
    for target in targets {
        if kind_of(target).iter().any(|kind| kind == "bin") {
            return target
                .get("name")
                .and_then(|value| value.as_str())
                .map(str::to_owned);
        }
    }
    None
}

/// Render the landing page. Inline CSS only; the sole external reference on
/// the whole doc site is the Redoc bundle inside `openapi.html`.
fn landing_html(entries: &[CrateEntry]) -> String {
    let mut rows = String::new();
    for entry in entries {
        let description = if entry.description.is_empty() {
            String::new()
        } else {
            format!(
                " <span class=\"desc\">— {}</span>",
                escape_html(&entry.description)
            )
        };
        rows.push_str(&format!(
            "      <li><a href=\"{dir}/index.html\"><code>{name}</code></a>{description}</li>\n",
            dir = escape_html(&entry.doc_dir),
            name = escape_html(&entry.name),
        ));
    }
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Soma API documentation</title>
    <style>
      :root {{ color-scheme: light dark; }}
      body {{
        font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
        max-width: 56rem; margin: 2rem auto; padding: 0 1.25rem;
        line-height: 1.55;
      }}
      h1 {{ margin-bottom: 0.25rem; }}
      .subtitle {{ color: color-mix(in srgb, currentColor 65%, transparent); }}
      a {{ color: #0969da; text-decoration: none; }}
      a:hover {{ text-decoration: underline; }}
      @media (prefers-color-scheme: dark) {{ a {{ color: #58a6ff; }} }}
      .card {{
        display: block; border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
        border-radius: 8px; padding: 0.9rem 1.1rem; margin: 1.25rem 0;
      }}
      .card strong {{ display: block; }}
      ul.crates {{ list-style: none; padding: 0; }}
      ul.crates li {{ padding: 0.3rem 0; border-bottom: 1px solid color-mix(in srgb, currentColor 12%, transparent); }}
      .desc {{ color: color-mix(in srgb, currentColor 65%, transparent); }}
      footer {{ margin-top: 2rem; font-size: 0.85rem; color: color-mix(in srgb, currentColor 55%, transparent); }}
    </style>
  </head>
  <body>
    <h1>Soma API documentation</h1>
    <p class="subtitle">Rust API reference for every crate in the soma workspace, built by <code>cargo xtask doc</code>.</p>
    <a class="card" href="openapi.html">
      <strong>REST API reference (OpenAPI)</strong>
      <span class="desc">The <code>/v1/*</code> HTTP contract from <code>docs/generated/openapi.json</code>, rendered with Redoc.</span>
    </a>
    <h2>Workspace crates</h2>
    <ul class="crates">
{rows}    </ul>
    <footer>Generated by <code>cargo xtask doc</code>; deployed from <code>.github/workflows/docs.yml</code>.</footer>
  </body>
</html>
"#
    )
}

/// Minimal HTML escaping for text interpolated into the landing page.
/// Descriptions come from our own Cargo manifests, but escaping keeps the
/// generator correct no matter what a future manifest says.
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Static Redoc host page. `spec-url` is relative so the page works both on
/// GitHub Pages and from a local `target/doc/` checkout; the standalone
/// bundle comes from the Redoc CDN (the doc site's only external asset).
const REDOC_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Soma REST API — OpenAPI reference</title>
    <style>
      body { margin: 0; padding: 0; }
      .back { font-family: system-ui, sans-serif; font-size: 0.9rem; padding: 0.5rem 1rem; }
    </style>
  </head>
  <body>
    <div class="back"><a href="index.html">&larr; All Soma API docs</a></div>
    <redoc spec-url="openapi.json"></redoc>
    <script src="https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js"></script>
  </body>
</html>
"#;
