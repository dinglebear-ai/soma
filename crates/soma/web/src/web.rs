//! Static web asset serving — embeds apps/web/out/ into the binary at compile time.
//!
//! When the `web` feature is enabled (default), `include_dir!` bakes every file from
//! `apps/web/out/` into the binary at compile time.  At runtime, `serve_web_assets`
//! responds to any HTTP request that wasn't matched by a more specific route (MCP, REST,
//! health, OAuth).
//!
//! The SPA fallback strategy:
//!   1. Serve the exact path if it exists          (e.g. `/favicon.ico`)
//!   2. Try path + `.html`                          (e.g. `/tools` → `tools.html`)
//!   3. Try path + `/index.html`                   (e.g. `/tools/` → `tools/index.html`)
//!   4. Fall back to `index.html` for client-side routing
//!
//! Cache-control:
//!   - HTML shells          → `no-store`  (routes must never be stale)
//!   - `_next/static/*`     → `public, max-age=31536000, immutable` (content-hashed)
//!   - Other assets         → `public, max-age=3600` (bounded cache)

#[cfg(feature = "web")]
use include_dir::{include_dir, Dir};
use include_dir::{include_dir as include_source_dir, Dir as SourceDir, DirEntry};
use std::{fs, path::Path};

#[cfg(feature = "web")]
static WEB_ASSETS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../../apps/web/out");
static WEB_SOURCE: SourceDir<'static> = include_source_dir!("$CARGO_MANIFEST_DIR/assets/source");

#[cfg(feature = "web")]
use axum::http::header;
use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// Returns `true` when the embedded `apps/web/out/index.html` is present.
///
/// Used by `AppState::web_assets_enabled()` and the router to decide whether to wire
/// up the SPA fallback handler.
pub fn web_assets_available() -> bool {
    #[cfg(feature = "web")]
    {
        WEB_ASSETS.get_file("index.html").is_some()
    }
    #[cfg(not(feature = "web"))]
    {
        false
    }
}

/// Returns `true` when the bundled editable Aurora frontend scaffold is present.
pub fn web_source_available() -> bool {
    WEB_SOURCE.get_file("package.json").is_some()
        && WEB_SOURCE.get_file("components/aurora.css").is_some()
        && WEB_SOURCE.get_file("app/page.tsx").is_some()
}

/// Copy the bundled editable Aurora frontend source into `destination`.
///
/// The bundle is intended for scaffolding a derived app's `apps/web` directory.
/// It contains source/config files only: no `node_modules`, `.next`, `out`, or
/// TypeScript build cache artifacts.
pub fn write_web_source_to(destination: impl AsRef<Path>, overwrite: bool) -> std::io::Result<()> {
    write_dir(&WEB_SOURCE, destination.as_ref(), overwrite)
}

/// Axum fallback handler — serves embedded static assets with SPA fallback.
pub async fn serve_web_assets(request: Request<Body>) -> Response {
    #[cfg(feature = "web")]
    {
        let path = normalize_asset_path(request.uri().path());

        // Ordered candidate list — first match wins.
        let candidates = asset_candidates(path);

        for candidate in candidates {
            if let Some(file) = WEB_ASSETS.get_file(candidate.as_str()) {
                let content_type = guess_mime(candidate.as_str());
                let cache_control = cache_control_for(candidate.as_str());
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, content_type),
                        (header::CACHE_CONTROL, cache_control),
                    ],
                    file.contents().to_vec(),
                )
                    .into_response();
            }
        }

        // SPA fallback — let client-side router handle the path
        if let Some(file) = WEB_ASSETS.get_file("index.html") {
            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                file.contents().to_vec(),
            )
                .into_response();
        }

        StatusCode::NOT_FOUND.into_response()
    }

    #[cfg(not(feature = "web"))]
    {
        let _ = request;
        StatusCode::NOT_FOUND.into_response()
    }
}

fn write_dir(dir: &SourceDir<'static>, destination: &Path, overwrite: bool) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in dir.entries() {
        let path = destination.join(entry.path());
        match entry {
            DirEntry::Dir(child) => {
                fs::create_dir_all(&path)?;
                write_dir(child, destination, overwrite)?;
            }
            DirEntry::File(file) => {
                if path.exists() && !overwrite {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        format!("{} already exists", path.display()),
                    ));
                }
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, file.contents())?;
            }
        }
    }
    Ok(())
}

#[cfg(any(feature = "web", test))]
fn normalize_asset_path(path: &str) -> &str {
    path.trim_start_matches('/').trim_end_matches('/')
}

#[cfg(any(feature = "web", test))]
fn asset_candidates(path: &str) -> Vec<String> {
    if path.is_empty() {
        return vec!["index.html".to_string()];
    }

    vec![
        path.to_string(),
        format!("{path}.html"),
        format!("{path}/index.html"),
    ]
}

#[cfg(any(feature = "web", test))]
fn cache_control_for(path: &str) -> &'static str {
    if path == "index.html" || path.ends_with(".html") {
        "no-store"
    } else if path.starts_with("_next/static/") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    }
}

#[cfg(any(feature = "web", test))]
fn guess_mime(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".js") || path.ends_with(".mjs") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else if path.ends_with(".woff") {
        "font/woff"
    } else if path.ends_with(".ttf") {
        "font/ttf"
    } else if path.ends_with(".txt") {
        "text/plain; charset=utf-8"
    } else if path.ends_with(".webmanifest") {
        "application/manifest+json"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;
