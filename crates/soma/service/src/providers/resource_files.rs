//! Providers backing structured `providers/resources/` files: one
//! `ResourceFileProvider` per discovered file, either serving static file
//! content directly or dispatching to a sandboxed TypeScript `read()`
//! reader — see `docs/contracts/drop-in-provider-layout.md`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::Value;
use soma_provider_core::{
    ProviderCatalog, ProviderIdentity, ProviderKind, ProviderManifest, ProviderResource,
};

use soma_provider_adapters::{error::SidecarError, sidecar::run_bounded_sidecar};

use crate::{
    provider_errors::ProviderError,
    provider_registry::{
        DynamicResourceTemplate, Provider, ProviderCall, ProviderOutput, ResourceReadOutput,
    },
    providers::resource_uri,
};

/// Static resource files larger than this are rejected at discovery time —
/// "enforce file size limits for static resources" per the layout contract.
pub const MAX_STATIC_RESOURCE_BYTES: u64 = 10 * 1024 * 1024;

const DYNAMIC_RESOURCE_TIMEOUT_MS: u64 = 10_000;
const DYNAMIC_RESOURCE_MAX_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
enum ResourceFileKind {
    Static {
        path: PathBuf,
        resource: Box<ProviderResource>,
        mime_type: String,
    },
    Dynamic {
        path: PathBuf,
        template: DynamicResourceTemplate,
    },
}

#[derive(Debug, Clone)]
pub struct ResourceFileProvider {
    provider_name: String,
    kind: ResourceFileKind,
    /// The canonicalized `providers/resources/` directory this file was
    /// discovered under, re-verified against on every read (not just at
    /// discovery time) to close the TOCTOU window a symlink swap could open
    /// between the directory walk and the eventual read/execute syscall.
    canonical_root: PathBuf,
}

impl ResourceFileProvider {
    /// Builds a provider for one file discovered under `providers/resources/`.
    /// `relative_path` is the file's path relative to that directory
    /// (already verified not to escape the provider root — see
    /// `filesystem::collect_resource_files`). `canonical_root` is that
    /// directory's canonicalized path, re-checked at read time. A `.ts`
    /// extension makes the file a dynamic reader; every other extension
    /// makes it a static resource read directly from disk.
    pub fn from_file(
        absolute_path: PathBuf,
        relative_path: &Path,
        canonical_root: &Path,
    ) -> Result<Self, ResourceFileError> {
        let is_dynamic_reader =
            relative_path.extension().and_then(|ext| ext.to_str()) == Some("ts");

        let mut stem_segments: Vec<String> = relative_path
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();
        if let Some(last) = stem_segments.last_mut() {
            if let Some(stem) = Path::new(last.as_str())
                .file_stem()
                .and_then(|stem| stem.to_str())
            {
                *last = stem.to_owned();
            }
        }
        let segment_refs: Vec<&str> = stem_segments.iter().map(String::as_str).collect();
        let resource_path = resource_uri::parse_resource_path(&segment_refs)
            .map_err(|error| ResourceFileError(error.0))?;
        // Bracket syntax ([name], [...name]) is meaningful in the filename
        // but must not leak into display strings — use the parsed segments'
        // clean values/param-names, not the raw stem, for anything the
        // provider-name schema pattern or a human might see.
        let clean_name = joined_segment_name(&resource_path);

        let provider_name = format!("resource-{clean_name}");

        if is_dynamic_reader {
            let description = format!(
                "Dynamic resource reader from {}",
                resource_uri::display_with_forward_slashes(relative_path)
            );
            let mut template = DynamicResourceTemplate::from_path_segments(
                &segment_refs,
                clean_name,
                description,
                None,
            )
            .map_err(ResourceFileError)?;
            // Dynamic readers execute arbitrary operator-authored code via
            // the Node sidecar (unlike static resources, which only ever
            // return file bytes) — default to the stricter write scope so a
            // read-scoped principal can't invoke them, since the drop-in
            // file convention has no manifest to declare a scope
            // explicitly. `write` satisfies `read` per `scopes_satisfy`, so
            // this only narrows who may call it, never widens it.
            template.scope = Some("soma:write".to_owned());
            return Ok(Self {
                provider_name,
                kind: ResourceFileKind::Dynamic {
                    path: absolute_path,
                    template,
                },
                canonical_root: canonical_root.to_owned(),
            });
        }

        let metadata = std::fs::metadata(&absolute_path).map_err(|source| {
            ResourceFileError(format!("failed to read resource file metadata: {source}"))
        })?;
        if metadata.len() > MAX_STATIC_RESOURCE_BYTES {
            return Err(ResourceFileError(format!(
                "resource file exceeds the {MAX_STATIC_RESOURCE_BYTES}-byte static resource limit"
            )));
        }
        if resource_path.is_dynamic() {
            return Err(ResourceFileError(
                "static resource files (non-.ts) cannot use bracketed [param] path segments"
                    .to_owned(),
            ));
        }

        let mime_type = mime_type_for_extension(relative_path);
        // The full-path clean_name, not just the leaf stem: two files that
        // share a leaf name under different directories (resources/api/runbook.md,
        // resources/ops/runbook.md) have distinct, valid, non-colliding URIs
        // but would collide on a leaf-only name, tripping the global
        // resource-name uniqueness check in build_snapshot() and failing
        // the whole refresh over two perfectly valid resources.
        let name = clean_name;
        let description = static_resource_description(&absolute_path, &mime_type, &name)?;
        let resource = ProviderResource {
            uri_template: resource_path.uri_string(),
            name,
            description,
            mime_type: Some(mime_type.clone()),
            scope: None,
            mcp: None,
            annotations: serde_json::json!({}),
        };
        Ok(Self {
            provider_name,
            kind: ResourceFileKind::Static {
                path: absolute_path,
                resource: Box::new(resource),
                mime_type,
            },
            canonical_root: canonical_root.to_owned(),
        })
    }

    pub fn arc(
        absolute_path: PathBuf,
        relative_path: &Path,
        canonical_root: &Path,
    ) -> Result<std::sync::Arc<dyn Provider>, ResourceFileError> {
        Ok(std::sync::Arc::new(Self::from_file(
            absolute_path,
            relative_path,
            canonical_root,
        )?))
    }
}

#[derive(Debug)]
pub struct ResourceFileError(pub String);

impl std::fmt::Display for ResourceFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ResourceFileError {}

/// Joins a parsed path's segment values for use as a display name: literal
/// segments contribute their slugified value, param/catch-all segments
/// contribute their (bracket-free) parameter name. Segments are joined with
/// `_`, never `-`: `slugify()` (used for every literal segment) only ever
/// produces `-` internally, so joining with the same character would make
/// nested paths and hyphenated flat names collide (`resources/my/file.md`
/// and `resources/my-file.md` would both flatten to `my-file`) even though
/// they map to different, non-colliding URIs — `_` keeps the two
/// unambiguous since it's schema-valid but never emitted by slugify.
fn joined_segment_name(path: &resource_uri::ResourcePath) -> String {
    path.segments
        .iter()
        .map(|segment| match segment {
            resource_uri::PathSegment::Literal(value) => value.clone(),
            resource_uri::PathSegment::Param(name) | resource_uri::PathSegment::CatchAll(name) => {
                name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("_")
}

fn static_resource_description(
    path: &Path,
    mime_type: &str,
    name: &str,
) -> Result<String, ResourceFileError> {
    if mime_type == "text/markdown" {
        let text = std::fs::read_to_string(path).map_err(|source| {
            ResourceFileError(format!("failed to read resource file: {source}"))
        })?;
        if let Some(heading) = first_markdown_heading(&text) {
            return Ok(heading);
        }
    }
    Ok(format!("Resource `{name}`"))
}

fn first_markdown_heading(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let trimmed = line.trim();
        let heading = trimmed.strip_prefix("# ")?.trim();
        (!heading.is_empty()).then(|| heading.to_owned())
    })
}

/// Small static extension table covering the layout contract's own
/// examples; unknown extensions fall back to `application/octet-stream`.
fn mime_type_for_extension(path: &Path) -> String {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "md" | "markdown" => "text/markdown",
        "txt" => "text/plain",
        "json" => "application/json",
        "yaml" | "yml" => "application/yaml",
        "toml" => "application/toml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
    .to_owned()
}

fn is_text_mime(mime_type: &str) -> bool {
    mime_type.starts_with("text/")
        || matches!(
            mime_type,
            "application/json" | "application/yaml" | "application/toml" | "application/xml"
        )
}

#[async_trait]
impl Provider for ResourceFileProvider {
    fn catalog(&self) -> ProviderCatalog {
        let (title, description, resources) = match &self.kind {
            ResourceFileKind::Static { resource, path, .. } => (
                resource.description.clone(),
                format!("Static resource file loaded from {}", path.display()),
                vec![resource.as_ref().clone()],
            ),
            ResourceFileKind::Dynamic { template, path } => (
                template.description.clone(),
                format!("Dynamic resource reader loaded from {}", path.display()),
                Vec::new(),
            ),
        };
        ProviderManifest {
            schema_version: 1,
            provider: ProviderIdentity {
                name: self.provider_name.clone(),
                kind: ProviderKind::StaticRust,
                title: Some(title),
                description: Some(description),
                homepage: None,
                source: Some(self.source_path().display().to_string()),
                version: None,
                enabled: None,
            },
            tools: Vec::new(),
            prompts: Vec::new(),
            resources,
            tasks: Vec::new(),
            elicitation: Vec::new(),
            env: Vec::new(),
            capabilities: Default::default(),
            docs: None,
            plugin: None,
            ui: None,
            meta: serde_json::json!({}),
        }
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        Err(ProviderError::validation(
            &self.provider_name,
            &call.action,
            "resource_provider_has_no_actions",
            "resource file providers do not expose any callable actions",
        ))
    }

    fn dynamic_resource_templates(&self) -> Vec<DynamicResourceTemplate> {
        match &self.kind {
            ResourceFileKind::Dynamic { template, .. } => vec![template.clone()],
            ResourceFileKind::Static { .. } => Vec::new(),
        }
    }

    fn supports_resource_reads(&self) -> bool {
        true
    }

    async fn read_resource(
        &self,
        uri: &str,
        params: &BTreeMap<String, String>,
    ) -> Result<ResourceReadOutput, ProviderError> {
        match &self.kind {
            ResourceFileKind::Static {
                path, mime_type, ..
            } => read_static_resource(
                &self.provider_name,
                uri,
                path,
                &self.canonical_root,
                mime_type,
            ),
            ResourceFileKind::Dynamic { path, .. } => {
                read_dynamic_resource(&self.provider_name, uri, path, &self.canonical_root, params)
                    .await
            }
        }
    }
}

impl ResourceFileProvider {
    fn source_path(&self) -> &Path {
        match &self.kind {
            ResourceFileKind::Static { path, .. } | ResourceFileKind::Dynamic { path, .. } => path,
        }
    }
}

/// Re-canonicalizes `path` and verifies it still resolves inside
/// `canonical_root`, closing the TOCTOU window between the directory walk
/// that validated a symlink's target at discovery time and this read: the
/// walk only checked the target once, but a symlink an attacker with local
/// write access controls could be swapped to point elsewhere afterward.
fn verify_within_root(
    provider_name: &str,
    uri: &str,
    path: &Path,
    canonical_root: &Path,
) -> Result<PathBuf, ProviderError> {
    let canonical = path.canonicalize().map_err(|source| {
        ProviderError::execution(
            provider_name,
            uri,
            format!("resource file unreadable: {source}"),
        )
    })?;
    if !canonical.starts_with(canonical_root) {
        return Err(ProviderError::validation(
            provider_name,
            uri,
            "resource_escapes_root",
            "resource path no longer resolves within the provider root",
        ));
    }
    Ok(canonical)
}

fn read_static_resource(
    provider_name: &str,
    uri: &str,
    path: &Path,
    canonical_root: &Path,
    mime_type: &str,
) -> Result<ResourceReadOutput, ProviderError> {
    let path = verify_within_root(provider_name, uri, path, canonical_root)?;
    let path = path.as_path();
    let metadata = std::fs::metadata(path).map_err(|source| {
        ProviderError::execution(
            provider_name,
            uri,
            format!("resource file unreadable: {source}"),
        )
    })?;
    if metadata.len() > MAX_STATIC_RESOURCE_BYTES {
        return Err(ProviderError::validation(
            provider_name,
            uri,
            "resource_too_large",
            format!("resource file exceeds the {MAX_STATIC_RESOURCE_BYTES}-byte limit"),
        ));
    }
    let bytes = std::fs::read(path).map_err(|source| {
        ProviderError::execution(
            provider_name,
            uri,
            format!("failed to read resource file: {source}"),
        )
    })?;
    if is_text_mime(mime_type) {
        let text = String::from_utf8(bytes).map_err(|error| {
            ProviderError::execution(
                provider_name,
                uri,
                format!("resource file is not valid UTF-8 text: {error}"),
            )
        })?;
        return Ok(ResourceReadOutput::Text {
            text,
            mime_type: Some(mime_type.to_owned()),
        });
    }
    Ok(ResourceReadOutput::Blob {
        blob_base64: BASE64.encode(bytes),
        mime_type: Some(mime_type.to_owned()),
    })
}

async fn read_dynamic_resource(
    provider_name: &str,
    uri: &str,
    path: &Path,
    canonical_root: &Path,
    params: &BTreeMap<String, String>,
) -> Result<ResourceReadOutput, ProviderError> {
    let canonical_path = verify_within_root(provider_name, uri, path, canonical_root)?;
    let query = resource_uri::query_params(uri);
    let input = serde_json::json!({
        "uri": uri,
        "params": params,
        "query": query,
    });
    let input_bytes = serde_json::to_vec(&input).map_err(|error| {
        ProviderError::execution(
            provider_name,
            uri,
            format!("failed to serialize reader input: {error}"),
        )
    })?;

    let wrapper = dynamic_reader_wrapper(&canonical_path);

    let sidecar = match run_bounded_sidecar(
        "node",
        &["--input-type=module", "--eval", &wrapper],
        Vec::new(),
        &input_bytes,
        DYNAMIC_RESOURCE_TIMEOUT_MS,
        DYNAMIC_RESOURCE_MAX_OUTPUT_BYTES,
    )
    .await
    {
        Ok(sidecar) => sidecar,
        Err(SidecarError::Timeout) => {
            return Err(ProviderError::new(
                "resource_reader_timeout",
                provider_name,
                Some(uri.to_owned()),
                format!("dynamic resource reader exceeded {DYNAMIC_RESOURCE_TIMEOUT_MS}ms timeout"),
                "Fix the reader's performance or reduce the work it does per call.",
            ));
        }
        Err(error) => {
            return Err(ProviderError::execution(provider_name, uri, error));
        }
    };

    if sidecar.stdout_exceeded || sidecar.stderr_exceeded {
        return Err(ProviderError::validation(
            provider_name,
            uri,
            "resource_reader_output_too_large",
            format!(
                "dynamic resource reader output exceeds {DYNAMIC_RESOURCE_MAX_OUTPUT_BYTES} bytes"
            ),
        ));
    }
    let output = sidecar.output;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ProviderError::new(
            "resource_reader_failed",
            provider_name,
            Some(uri.to_owned()),
            format!("dynamic resource reader failed: {stderr}"),
            "Fix the TypeScript resource reader and retry.",
        ));
    }

    let value: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        ProviderError::validation(
            provider_name,
            uri,
            "resource_reader_invalid_json_output",
            error.to_string(),
        )
    })?;
    parse_reader_output(provider_name, uri, &value)
}

fn parse_reader_output(
    provider_name: &str,
    uri: &str,
    value: &Value,
) -> Result<ResourceReadOutput, ProviderError> {
    let invalid = |message: &str| {
        ProviderError::validation(
            provider_name,
            uri,
            "resource_reader_invalid_shape",
            message.to_owned(),
        )
    };
    let object = value
        .as_object()
        .ok_or_else(|| invalid("reader must return an object"))?;

    if let Some(text) = object.get("text") {
        let text = text
            .as_str()
            .ok_or_else(|| invalid("`text` must be a string"))?
            .to_owned();
        let mime_type = object
            .get("mimeType")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        return Ok(ResourceReadOutput::Text { text, mime_type });
    }
    if let Some(json_value) = object.get("json") {
        let text = serde_json::to_string(json_value)
            .map_err(|error| invalid(&format!("`json` result could not be serialized: {error}")))?;
        return Ok(ResourceReadOutput::Text {
            text,
            mime_type: Some("application/json".to_owned()),
        });
    }
    if let Some(blob) = object.get("blob") {
        let blob_base64 = blob
            .as_str()
            .ok_or_else(|| invalid("`blob` must be a base64 string"))?
            .to_owned();
        let mime_type = object
            .get("mimeType")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("`blob` results require a `mimeType`"))?
            .to_owned();
        return Ok(ResourceReadOutput::Blob {
            blob_base64,
            mime_type: Some(mime_type),
        });
    }
    Err(invalid(
        "reader must return one of `{ text }`, `{ json }`, or `{ blob, mimeType }`",
    ))
}

/// Builds the sidecar's JS source for a reader whose module path has
/// already been canonicalized and verified within the resources root by the
/// caller (`read_dynamic_resource`) — this function only formats it.
/// `module_path` is embedded via `serde_json::to_string` rather than Rust's
/// `{:?}` Debug formatting: both happen to agree on how to escape a plain
/// path today, but only the former is a documented guarantee of producing a
/// valid JS/JSON string literal.
fn dynamic_reader_wrapper(canonical_path: &Path) -> String {
    let module_path = serde_json::to_string(&canonical_path.display().to_string())
        .expect("String serialization to JSON cannot fail");
    format!(
        r#"
import {{ readFileSync }} from "node:fs";
const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const input = JSON.parse(Buffer.concat(chunks).toString("utf8") || "{{}}");
for (const key of Object.keys(process.env)) {{
  delete process.env[key];
}}
const readerSource = readFileSync({module_path}, "utf8");
const module = await import("data:text/javascript;base64," + Buffer.from(readerSource).toString("base64"));
const handler = module.read;
if (typeof handler !== "function") {{
  throw new Error("Dynamic resource reader must export async function read(input)");
}}
const result = await handler(input);
process.stdout.write(JSON.stringify(result ?? null));
"#
    )
}

#[cfg(test)]
#[path = "resource_files_tests.rs"]
mod tests;
