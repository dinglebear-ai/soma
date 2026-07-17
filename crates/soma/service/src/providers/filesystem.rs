use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;
use sha2::{Digest, Sha256};
use soma_domain::provider_validation::{validate_manifest_schema, validate_provider_manifest};
use soma_provider_adapters::manifest_file;
use soma_provider_core::{ProviderCatalog, ProviderKind};

use crate::{
    provider_registry::{DynamicResourceTemplate, Provider, SharedAdapter},
    providers::resource_files::{ResourceFileError, ResourceFileProvider},
};

/// Product env-namespace forwarded to shared adapters that resolve
/// `EnvRequirement`s (ai-sdk, python) — see `soma_provider_adapters::sidecar
/// ::collect_provider_env`'s docs for why this crate has no hard-coded
/// product prefix of its own.
const PROVIDER_ENV_PREFIX: &str = "SOMA";

#[path = "filesystem_prompts.rs"]
mod filesystem_prompts;
#[path = "filesystem_resources.rs"]
mod filesystem_resources;
#[path = "filesystem_uniqueness.rs"]
mod filesystem_uniqueness;
#[path = "filesystem_wasm.rs"]
mod filesystem_wasm;

#[derive(Debug, Clone)]
pub struct FileProviderSource {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDirectoryInspection {
    pub root: PathBuf,
    pub exists: bool,
    pub files: Vec<ProviderFileInspection>,
    pub providers_loaded: usize,
    pub providers_disabled: usize,
    pub providers_invalid: usize,
    pub providers_skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderFileInspection {
    pub path: PathBuf,
    pub file_name: String,
    pub status: ProviderFileInspectionStatus,
    pub provider_id: Option<String>,
    pub provider_kind: Option<String>,
    pub actions: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderFileInspectionStatus {
    Loaded,
    Disabled,
    Invalid,
    /// File extension requires executing code to introspect (currently just
    /// `.py`) — non-executing inspection deliberately does not load it.
    Skipped,
}

impl FileProviderSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Non-executing inspection of the provider directory: parses manifests
    /// (JSON/TS/WASM sidecar/Python/Markdown) but never runs handler code,
    /// calls MCP, or fetches OpenAPI — safe to run before the runtime loads
    /// providers.
    pub fn inspect(&self) -> Result<ProviderDirectoryInspection, FileProviderLoadError> {
        if !self.root.exists() {
            return Ok(ProviderDirectoryInspection {
                root: self.root.clone(),
                exists: false,
                files: Vec::new(),
                providers_loaded: 0,
                providers_disabled: 0,
                providers_invalid: 0,
                providers_skipped: 0,
            });
        }

        let mut files = Vec::new();
        // Parallel to `files`, index-aligned: the parsed catalog for any file
        // that is (so far) `Loaded`, used by the directory-wide uniqueness
        // pass below. `Disabled` catalogs are intentionally excluded here —
        // `load()` never registers disabled providers either, so they can't
        // collide with anything at runtime.
        let mut loaded_catalogs: Vec<Option<ProviderCatalog>> = Vec::new();
        // Parallel to `files`/`loaded_catalogs`: a dynamic `.ts` resource
        // reader's template, which the live `ResourceIndex::register`
        // checks for ambiguity but which never appears in `catalog().resources`
        // (dynamic templates aren't declared data, they're derived from the
        // filename) — without this, lint can't see two colliding readers
        // like `service/[name].ts` and `service/[id].ts` at all.
        let mut dynamic_templates: Vec<Option<DynamicResourceTemplate>> = Vec::new();

        for path in self.provider_paths()? {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<unknown>")
                .to_owned();

            // Python catalogs are extracted by importing (and thus executing) the
            // module in a sidecar process — there is no metadata-only path. Never
            // run that from a non-executing inspection; report it as skipped
            // instead of silently exec'ing arbitrary import-time code.
            if is_python_provider_source(&path) {
                files.push(ProviderFileInspection {
                    path,
                    file_name,
                    status: ProviderFileInspectionStatus::Skipped,
                    provider_id: None,
                    provider_kind: Some(ProviderKind::Python.as_str().to_owned()),
                    actions: Vec::new(),
                    error: Some(
                        "Python providers can only be introspected by executing the module; \
                         non-executing inspection does not run them. Use `soma providers \
                         validate` or `soma providers inspect` to check this file."
                            .to_owned(),
                    ),
                });
                loaded_catalogs.push(None);
                continue;
            }

            match load_catalog(&path) {
                Ok(catalog) => {
                    let semantic_check = load_catalog_value(&path)
                        .map_err(|error| error.to_string())
                        .and_then(|value| {
                            validate_manifest_schema(&value).map_err(|error| error.to_string())
                        })
                        .and_then(|()| {
                            validate_provider_manifest(&catalog).map_err(|error| error.to_string())
                        })
                        .and_then(|()| compile_tool_schemas(&catalog));
                    match semantic_check {
                        Ok(()) => {
                            let status = if catalog.provider.enabled == Some(false) {
                                ProviderFileInspectionStatus::Disabled
                            } else {
                                ProviderFileInspectionStatus::Loaded
                            };
                            let actions = catalog
                                .tools
                                .iter()
                                .map(|tool| tool.name.clone())
                                .collect::<Vec<_>>();
                            files.push(ProviderFileInspection {
                                path,
                                file_name,
                                status,
                                provider_id: Some(catalog.provider.name.clone()),
                                provider_kind: Some(catalog.provider.kind.as_str().to_owned()),
                                actions,
                                error: None,
                            });
                            loaded_catalogs.push(
                                (status == ProviderFileInspectionStatus::Loaded).then_some(catalog),
                            );
                        }
                        Err(message) => {
                            files.push(ProviderFileInspection {
                                path,
                                file_name,
                                status: ProviderFileInspectionStatus::Invalid,
                                provider_id: Some(catalog.provider.name),
                                provider_kind: Some(catalog.provider.kind.as_str().to_owned()),
                                actions: Vec::new(),
                                error: Some(message),
                            });
                            loaded_catalogs.push(None);
                        }
                    }
                }
                Err(error) => {
                    files.push(ProviderFileInspection {
                        path,
                        file_name,
                        status: ProviderFileInspectionStatus::Invalid,
                        provider_id: None,
                        provider_kind: None,
                        actions: Vec::new(),
                        error: Some(error.to_string()),
                    });
                    loaded_catalogs.push(None);
                }
            }
        }

        // None of the entries pushed above are resource files, so pad
        // `dynamic_templates` up to the same length before extending with
        // the resource files' own (possibly `Some`) entries below — keeps
        // all three vectors index-aligned with `files` without touching
        // every earlier push site.
        dynamic_templates.resize(files.len(), None);
        let (resource_files, resource_catalogs, resource_templates) =
            filesystem_resources::inspect_files(self.resource_pairs_with_canonical_root()?);
        files.extend(resource_files);
        loaded_catalogs.extend(resource_catalogs);
        dynamic_templates.extend(resource_templates);

        filesystem_uniqueness::apply_directory_wide_checks(
            &mut files,
            &loaded_catalogs,
            &dynamic_templates,
        );
        files.sort_by(|left, right| left.file_name.cmp(&right.file_name));
        let providers_loaded = files
            .iter()
            .filter(|file| file.status == ProviderFileInspectionStatus::Loaded)
            .count();
        let providers_disabled = files
            .iter()
            .filter(|file| file.status == ProviderFileInspectionStatus::Disabled)
            .count();
        let providers_invalid = files
            .iter()
            .filter(|file| file.status == ProviderFileInspectionStatus::Invalid)
            .count();
        let providers_skipped = files
            .iter()
            .filter(|file| file.status == ProviderFileInspectionStatus::Skipped)
            .count();

        Ok(ProviderDirectoryInspection {
            root: self.root.clone(),
            exists: true,
            files,
            providers_loaded,
            providers_disabled,
            providers_invalid,
            providers_skipped,
        })
    }

    pub fn load(&self) -> Result<Vec<std::sync::Arc<dyn Provider>>, FileProviderLoadError> {
        let mut providers = Vec::new();
        for path in self.provider_paths()? {
            let catalog = load_catalog(&path)?;
            if catalog.provider.enabled == Some(false) {
                continue;
            }
            providers.push(provider_for_catalog(path, catalog)?);
        }
        for (absolute, relative, canonical_root) in self.resource_pairs_with_canonical_root()? {
            let provider = ResourceFileProvider::arc(absolute.clone(), &relative, &canonical_root)
                .map_err(|ResourceFileError(message)| FileProviderLoadError {
                    path: absolute,
                    message,
                })?;
            providers.push(provider);
        }
        Ok(providers)
    }

    pub fn fingerprint(&self) -> Result<String, FileProviderLoadError> {
        let mut hasher = Sha256::new();
        for path in self.fingerprint_paths()? {
            fingerprint_file(&mut hasher, &self.root, &path)?;
        }
        Ok(hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>())
    }

    fn fingerprint_paths(&self) -> Result<Vec<PathBuf>, FileProviderLoadError> {
        let provider_paths = self.provider_paths()?;
        let mut paths = BTreeSet::new();
        let mut has_python_provider = false;

        for path in &provider_paths {
            match path.extension().and_then(|extension| extension.to_str()) {
                Some("wasm") => {
                    let sidecar = filesystem_wasm::wasm_sidecar_manifest_path(path);
                    if sidecar.is_file() {
                        paths.insert(sidecar);
                    } else {
                        paths.insert(path.clone());
                    }
                }
                Some("py") => {
                    has_python_provider = true;
                    paths.insert(path.clone());
                }
                _ => {
                    paths.insert(path.clone());
                }
            }
        }

        if has_python_provider {
            collect_python_dependency_paths(&self.root, &mut paths)?;
        }

        for (absolute, _relative) in self.resource_paths()? {
            paths.insert(absolute);
        }

        Ok(paths.into_iter().collect())
    }

    /// Manifest-backed provider files (`.json`/`.ts`/`.wasm`/`.py`/`.md`),
    /// from the provider root plus the structured `tools/` and `prompts/`
    /// subdirectories, if present. Root-level files remain supported for
    /// compatibility per the drop-in provider layout contract; new docs and
    /// examples should prefer the structured layout. Neither subdirectory is
    /// scanned recursively — same flat-directory semantics as root.
    fn provider_paths(&self) -> Result<Vec<PathBuf>, FileProviderLoadError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut paths = Vec::new();
        collect_flat_files(&self.root, &mut paths, |path| {
            is_provider_file(path) && !is_wasm_sidecar_manifest(path)
        })?;
        let tools_dir = self.root.join("tools");
        if tools_dir.is_dir() {
            collect_flat_files(&tools_dir, &mut paths, |path| {
                is_tool_file(path) && !is_wasm_sidecar_manifest(path)
            })?;
        }
        let prompts_dir = self.root.join("prompts");
        if prompts_dir.is_dir() {
            collect_flat_files(&prompts_dir, &mut paths, is_markdown_prompt_file)?;
        }
        paths.sort();
        Ok(paths)
    }

    /// Files under the structured `resources/` subdirectory, recursively,
    /// as `(absolute_path, path_relative_to_resources_dir)` pairs. See
    /// `filesystem_resources::resource_paths` for the trust-boundary
    /// enforcement this delegates to.
    fn resource_paths(&self) -> Result<Vec<(PathBuf, PathBuf)>, FileProviderLoadError> {
        filesystem_resources::resource_paths(&self.root)
    }

    /// `resource_paths()` triples with the canonicalized `resources/`
    /// directory attached to each, so `ResourceFileProvider` can re-verify
    /// containment at read time against the same root discovery validated,
    /// closing the TOCTOU window between the two.
    fn resource_pairs_with_canonical_root(
        &self,
    ) -> Result<Vec<(PathBuf, PathBuf, PathBuf)>, FileProviderLoadError> {
        let pairs = self.resource_paths()?;
        if pairs.is_empty() {
            return Ok(Vec::new());
        }
        let canonical_root = filesystem_resources::canonical_resources_root(&self.root)?
            .expect("non-empty resource_paths() implies the resources dir exists");
        Ok(pairs
            .into_iter()
            .map(|(absolute, relative)| (absolute, relative, canonical_root.clone()))
            .collect())
    }
}

fn collect_flat_files(
    dir: &Path,
    paths: &mut Vec<PathBuf>,
    accept: impl Fn(&Path) -> bool,
) -> Result<(), FileProviderLoadError> {
    let entries = fs::read_dir(dir).map_err(|source| FileProviderLoadError {
        path: dir.to_path_buf(),
        message: format!("failed to read provider directory: {source}"),
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| FileProviderLoadError {
            path: dir.to_path_buf(),
            message: format!("failed to read provider directory entry: {source}"),
        })?;
        let path = entry.path();
        if path.is_file() && accept(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

fn collect_python_dependency_paths(
    root: &Path,
    paths: &mut BTreeSet<PathBuf>,
) -> Result<(), FileProviderLoadError> {
    if !root.exists() {
        return Ok(());
    }
    collect_python_dependency_paths_inner(root, paths)
}

fn collect_python_dependency_paths_inner(
    dir: &Path,
    paths: &mut BTreeSet<PathBuf>,
) -> Result<(), FileProviderLoadError> {
    let entries = fs::read_dir(dir).map_err(|source| FileProviderLoadError {
        path: dir.to_path_buf(),
        message: format!("failed to read provider dependency directory: {source}"),
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| FileProviderLoadError {
            path: dir.to_path_buf(),
            message: format!("failed to read provider dependency directory entry: {source}"),
        })?;
        let path = entry.path();
        if path.is_dir() {
            if should_scan_dependency_dir(&path) {
                collect_python_dependency_paths_inner(&path, paths)?;
            }
            continue;
        }
        if path.is_file() && is_python_dependency_file(&path) {
            paths.insert(path);
        }
    }
    Ok(())
}

fn should_scan_dependency_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    !matches!(
        name,
        "__pycache__"
            | ".git"
            | ".mypy_cache"
            | ".pytest_cache"
            | ".ruff_cache"
            | ".venv"
            | "venv"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
    )
}

fn is_python_dependency_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("py" | "pyi")
    )
}

#[derive(Debug)]
pub struct FileProviderLoadError {
    pub path: PathBuf,
    pub message: String,
}

impl std::fmt::Display for FileProviderLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for FileProviderLoadError {}

/// Builds the concrete provider for `catalog`'s declared kind. Every kind's
/// actual implementation lives in the product-neutral `soma-provider-adapters`
/// crate (feature-gated per kind); this just dispatches to it and wraps the
/// result to satisfy soma-service's own `Provider` trait — see
/// `provider_registry::SharedAdapter` and the PR10 deviation notes on why
/// this dispatch step, and the directory-scanning/Soma-policy orchestration
/// around it, stayed in soma-service rather than moving wholesale.
///
/// `soma-service` currently enables every `soma-provider-adapters` kind
/// feature (see its `Cargo.toml`), so `manifest_file::build_provider`
/// returning `None` — meaning this binary was built without the feature that
/// owns `catalog`'s kind — should never happen today. It is nonetheless
/// treated as an ordinary, per-manifest `FileProviderLoadError` rather than a
/// process-crashing `unreachable!()`: that invariant depends on two files (a
/// `Cargo.toml` feature list and this crate's `ProviderKind` coverage)
/// staying in sync by convention, with nothing enforcing it at compile time.
/// If they ever drift, one misconfigured/unsupported provider manifest
/// should fail to load, not take down every other already-working provider.
fn provider_for_catalog(
    path: PathBuf,
    catalog: ProviderCatalog,
) -> Result<std::sync::Arc<dyn Provider>, FileProviderLoadError> {
    let kind = catalog.provider.kind;
    manifest_file::build_provider(path.clone(), catalog, PROVIDER_ENV_PREFIX)
        .map(SharedAdapter::wrap)
        .ok_or_else(|| FileProviderLoadError {
            path,
            message: format!(
                "provider kind `{}` is not enabled in this build (soma-provider-adapters feature missing)",
                kind.as_str()
            ),
        })
}

fn is_provider_file(path: &Path) -> bool {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("json" | "ts" | "wasm" | "py") => true,
        Some("md") => is_markdown_prompt_file(path),
        _ => false,
    }
}

/// The structured `providers/tools/` directory only owns action-like files —
/// no `.md`, which belongs to `providers/prompts/` instead.
fn is_tool_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("json" | "ts" | "wasm" | "py")
    )
}

/// A `.md` file is a prompt provider unless it's the directory's own README —
/// `examples/providers/README.md` documents the directory, it isn't a prompt.
fn is_markdown_prompt_file(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| !stem.eq_ignore_ascii_case("readme"))
        .unwrap_or(false)
}

fn is_wasm_sidecar_manifest(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".wasm.json"))
}

fn is_python_provider_source(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("py")
}

/// Mirrors the schema-compilation pass `provider_registry::build_snapshot()`
/// runs for every tool — a manifest can deserialize and pass
/// `validate_provider_manifest()` while still carrying an `input_schema` or
/// `output_schema` that fails to compile as JSON Schema (e.g. `properties`
/// given as an array instead of an object). Non-executing inspection must
/// catch that too, or `lint` can bless a provider the live registry rejects.
fn compile_tool_schemas(catalog: &ProviderCatalog) -> Result<(), String> {
    for tool in &catalog.tools {
        jsonschema::validator_for(&tool.input_schema)
            .map_err(|error| format!("tool `{}` has invalid input_schema: {error}", tool.name))?;
        if let Some(output_schema) = &tool.output_schema {
            jsonschema::validator_for(output_schema).map_err(|error| {
                format!("tool `{}` has invalid output_schema: {error}", tool.name)
            })?;
        }
    }
    Ok(())
}

fn fingerprint_file(
    hasher: &mut Sha256,
    root: &Path,
    path: &Path,
) -> Result<(), FileProviderLoadError> {
    let bytes = fs::read(path).map_err(|source| FileProviderLoadError {
        path: path.to_path_buf(),
        message: format!("failed to read provider file for fingerprint: {source}"),
    })?;
    let label = path
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string();
    hasher.update(label.as_bytes());
    hasher.update([0]);
    hasher.update(bytes.len().to_le_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    hasher.update([0xff]);
    Ok(())
}

fn load_catalog(path: &Path) -> Result<ProviderCatalog, FileProviderLoadError> {
    let extension = path.extension().and_then(|extension| extension.to_str());
    let catalog = match extension {
        Some("json") | Some("ts") | Some("wasm") | Some("md") => {
            let value = load_catalog_value(path)?;
            serde_json::from_value(value).map_err(|source| FileProviderLoadError {
                path: path.to_path_buf(),
                message: format!("invalid provider manifest JSON: {source}"),
            })?
        }
        Some("py") => {
            // Only generic (soma-provider-core) manifest validation runs
            // here — Soma's own CLI reserved-command / env-prefix policy is
            // enforced uniformly for every provider kind by
            // `provider_registry::build_registry`'s
            // `validate_provider_manifest(&provider.catalog())` call, so
            // this does not skip that policy, just defers it to the same
            // place every other kind already goes through.
            soma_provider_adapters::python::load_python_catalog(path, PROVIDER_ENV_PREFIX).map_err(
                |source| FileProviderLoadError {
                    path: path.to_path_buf(),
                    message: format!("invalid Python provider: {source}"),
                },
            )?
        }
        _ => {
            return Err(FileProviderLoadError {
                path: path.to_path_buf(),
                message: "unsupported provider file extension".to_owned(),
            });
        }
    };
    ensure_kind_matches(path, &catalog)?;
    Ok(catalog)
}

/// Parses a JSON/TS/WASM-sidecar provider file to a raw `Value`, one step
/// short of `load_catalog`'s typed `ProviderCatalog`. Used by non-executing
/// inspection to validate against `provider-manifest.schema.json` (schema-only
/// constraints like `rest.path`'s pattern) before that information is lost to
/// `#[serde(default)]` fields round-tripping through `Option::None` as JSON
/// `null`, which the schema — correctly — does not accept in place of an
/// absent key.
fn load_catalog_value(path: &Path) -> Result<Value, FileProviderLoadError> {
    let extension = path.extension().and_then(|extension| extension.to_str());
    match extension {
        Some("json") => {
            serde_json::from_slice(&fs::read(path).map_err(|source| FileProviderLoadError {
                path: path.to_path_buf(),
                message: format!("failed to read provider manifest: {source}"),
            })?)
            .map_err(|source| FileProviderLoadError {
                path: path.to_path_buf(),
                message: format!("invalid provider manifest JSON: {source}"),
            })
        }
        Some("ts") => load_ts_catalog_value(path),
        Some("wasm") => filesystem_wasm::load_wasm_catalog_value(path),
        Some("md") => filesystem_prompts::load_markdown_catalog_value(path),
        _ => Err(FileProviderLoadError {
            path: path.to_path_buf(),
            message: "unsupported provider file extension".to_owned(),
        }),
    }
}

fn load_ts_catalog_value(path: &Path) -> Result<Value, FileProviderLoadError> {
    let text = fs::read_to_string(path).map_err(|source| FileProviderLoadError {
        path: path.to_path_buf(),
        message: format!("failed to read TypeScript provider: {source}"),
    })?;
    let json_text = extract_ts_manifest(&text).ok_or_else(|| FileProviderLoadError {
        path: path.to_path_buf(),
        message: "TypeScript provider must contain `export default { ... }` manifest JSON"
            .to_owned(),
    })?;
    serde_json::from_str(json_text).map_err(|source| FileProviderLoadError {
        path: path.to_path_buf(),
        message: format!("invalid TypeScript provider manifest JSON: {source}"),
    })
}

fn extract_ts_manifest(text: &str) -> Option<&str> {
    let marker = "export default";
    let start = text.find(marker)? + marker.len();
    let rest = text[start..].trim_start();
    let open = rest.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in rest[open..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let end = open + offset + ch.len_utf8();
                    return Some(rest[..end].trim());
                }
            }
            _ => {}
        }
    }
    None
}

fn ensure_kind_matches(
    path: &Path,
    catalog: &ProviderCatalog,
) -> Result<(), FileProviderLoadError> {
    let extension = path.extension().and_then(|extension| extension.to_str());
    let required_extension = required_extension_for_kind(catalog.provider.kind);
    if required_extension.is_some_and(|expected| extension != Some(expected)) {
        return Err(FileProviderLoadError {
            path: path.to_path_buf(),
            message: format!(
                "provider kind `{}` requires a .{} file",
                catalog.provider.kind.as_str(),
                required_extension_for_kind(catalog.provider.kind).unwrap()
            ),
        });
    }

    // No `Some("md")` arm: `load_markdown_catalog_value` unconditionally
    // hardcodes `"kind": "static-rust"`, so a mismatch can't currently occur.
    // `.md` catalogs fall through to `_ => {}` like any other `static-rust`
    // manifest (`required_extension_for_kind` returns `None` for it too).
    match extension {
        Some("ts") if catalog.provider.kind != ProviderKind::AiSdk => {
            return Err(FileProviderLoadError {
                path: path.to_path_buf(),
                message: format!(
                    "provider kind `{}` does not match TypeScript provider extension",
                    catalog.provider.kind.as_str()
                ),
            });
        }
        Some("wasm") if catalog.provider.kind != ProviderKind::Wasm => {
            return Err(FileProviderLoadError {
                path: path.to_path_buf(),
                message: format!(
                    "provider kind `{}` does not match WASM provider extension",
                    catalog.provider.kind.as_str()
                ),
            });
        }
        Some("py")
            if !matches!(
                catalog.provider.kind,
                ProviderKind::Python | ProviderKind::Langchain | ProviderKind::Llamaindex
            ) =>
        {
            return Err(FileProviderLoadError {
                path: path.to_path_buf(),
                message: format!(
                    "provider kind `{}` does not match Python provider extension",
                    catalog.provider.kind.as_str()
                ),
            });
        }
        _ => {}
    }
    Ok(())
}

fn required_extension_for_kind(kind: ProviderKind) -> Option<&'static str> {
    match kind {
        ProviderKind::AiSdk => Some("ts"),
        ProviderKind::Wasm => Some("wasm"),
        ProviderKind::Python | ProviderKind::Langchain | ProviderKind::Llamaindex => Some("py"),
        ProviderKind::StaticRust | ProviderKind::Openapi | ProviderKind::Mcp => None,
    }
}

#[cfg(test)]
#[path = "filesystem_tests.rs"]
mod tests;
