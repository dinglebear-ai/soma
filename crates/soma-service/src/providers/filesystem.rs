use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use serde_json::json;
use sha2::{Digest, Sha256};
use soma_contracts::{
    provider_validation::validate_provider_manifest,
    providers::{ProviderCatalog, ProviderKind},
};

use crate::{
    provider_errors::ProviderError,
    provider_registry::{Provider, ProviderCall, ProviderOutput},
    providers::{
        ai_sdk::AiSdkProvider,
        mcp::McpProvider,
        openapi::OpenApiProvider,
        python::{load_python_catalog, PythonProvider},
        wasm::WasmProvider,
    },
};

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
    /// (JSON/TS/WASM sidecar/Python) but never runs handler code, calls MCP,
    /// or fetches OpenAPI — safe to run before the runtime loads providers.
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

        let entries = fs::read_dir(&self.root).map_err(|source| FileProviderLoadError {
            path: self.root.clone(),
            message: format!("failed to read provider directory: {source}"),
        })?;
        let mut files = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|source| FileProviderLoadError {
                path: self.root.clone(),
                message: format!("failed to read provider directory entry: {source}"),
            })?;
            let path = entry.path();
            if !path.is_file() || !is_provider_file(&path) || is_wasm_sidecar_manifest(&path) {
                continue;
            }

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
                continue;
            }

            match load_catalog(&path) {
                Ok(catalog) => match validate_provider_manifest(&catalog) {
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
                            provider_id: Some(catalog.provider.name),
                            provider_kind: Some(catalog.provider.kind.as_str().to_owned()),
                            actions,
                            error: None,
                        });
                    }
                    Err(validation_error) => files.push(ProviderFileInspection {
                        path,
                        file_name,
                        status: ProviderFileInspectionStatus::Invalid,
                        provider_id: Some(catalog.provider.name),
                        provider_kind: Some(catalog.provider.kind.as_str().to_owned()),
                        actions: Vec::new(),
                        error: Some(validation_error.to_string()),
                    }),
                },
                Err(error) => files.push(ProviderFileInspection {
                    path,
                    file_name,
                    status: ProviderFileInspectionStatus::Invalid,
                    provider_id: None,
                    provider_kind: None,
                    actions: Vec::new(),
                    error: Some(error.to_string()),
                }),
            }
        }

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
            providers.push(provider_for_catalog(path, catalog));
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
                    let sidecar = wasm_sidecar_manifest_path(path);
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

        Ok(paths.into_iter().collect())
    }

    fn provider_paths(&self) -> Result<Vec<PathBuf>, FileProviderLoadError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let entries = fs::read_dir(&self.root).map_err(|source| FileProviderLoadError {
            path: self.root.clone(),
            message: format!("failed to read provider directory: {source}"),
        })?;
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| FileProviderLoadError {
                path: self.root.clone(),
                message: format!("failed to read provider directory entry: {source}"),
            })?;
            let path = entry.path();
            if path.is_file() && is_provider_file(&path) && !is_wasm_sidecar_manifest(&path) {
                paths.push(path);
            }
        }
        paths.sort();
        Ok(paths)
    }
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

#[derive(Clone)]
struct FileProvider {
    path: PathBuf,
    catalog: ProviderCatalog,
}

fn provider_for_catalog(path: PathBuf, catalog: ProviderCatalog) -> std::sync::Arc<dyn Provider> {
    match catalog.provider.kind {
        ProviderKind::Openapi => OpenApiProvider::arc(catalog),
        ProviderKind::Mcp => McpProvider::arc(catalog),
        ProviderKind::AiSdk => AiSdkProvider::arc(path, catalog),
        ProviderKind::Wasm => WasmProvider::arc(path, catalog),
        ProviderKind::Python | ProviderKind::Langchain | ProviderKind::Llamaindex => {
            PythonProvider::arc(path, catalog)
        }
        ProviderKind::StaticRust => std::sync::Arc::new(FileProvider { path, catalog }),
    }
}

#[async_trait]
impl Provider for FileProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let tool = self
            .catalog
            .tools
            .iter()
            .find(|tool| tool.name == call.action)
            .ok_or_else(|| {
                ProviderError::validation(
                    &self.catalog.provider.name,
                    &call.action,
                    "unknown_file_provider_action",
                    format!(
                        "provider file `{}` does not expose this action",
                        self.path.display()
                    ),
                )
            })?;

        if let Some(result) = tool.meta.get("result").cloned() {
            return Ok(ProviderOutput::json(result));
        }

        Ok(ProviderOutput::json(json!({
            "kind": "file_provider_result",
            "schema_version": 1,
            "provider": self.catalog.provider.name,
            "provider_kind": self.catalog.provider.kind.as_str(),
            "action": call.action,
            "params": call.params,
            "source": self.path.display().to_string(),
        })))
    }
}

fn is_provider_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("json" | "ts" | "wasm" | "py")
    )
}

fn is_wasm_sidecar_manifest(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".wasm.json"))
}

fn is_python_provider_source(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("py")
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
        Some("json") => {
            serde_json::from_slice(&fs::read(path).map_err(|source| FileProviderLoadError {
                path: path.to_path_buf(),
                message: format!("failed to read provider manifest: {source}"),
            })?)
            .map_err(|source| FileProviderLoadError {
                path: path.to_path_buf(),
                message: format!("invalid provider manifest JSON: {source}"),
            })?
        }
        Some("ts") => load_ts_catalog(path)?,
        Some("wasm") => load_wasm_catalog(path)?,
        Some("py") => load_python_catalog(path).map_err(|source| FileProviderLoadError {
            path: path.to_path_buf(),
            message: format!("invalid Python provider: {source}"),
        })?,
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

fn load_ts_catalog(path: &Path) -> Result<ProviderCatalog, FileProviderLoadError> {
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

fn load_wasm_catalog(path: &Path) -> Result<ProviderCatalog, FileProviderLoadError> {
    let sidecar_path = wasm_sidecar_manifest_path(path);
    if sidecar_path.is_file() {
        return serde_json::from_slice(&fs::read(&sidecar_path).map_err(|source| {
            FileProviderLoadError {
                path: sidecar_path.clone(),
                message: format!("failed to read WASM provider sidecar manifest: {source}"),
            }
        })?)
        .map_err(|source| FileProviderLoadError {
            path: sidecar_path,
            message: format!("invalid WASM provider sidecar manifest JSON: {source}"),
        });
    }

    let bytes = fs::read(path).map_err(|source| FileProviderLoadError {
        path: path.to_path_buf(),
        message: format!("failed to read WASM provider: {source}"),
    })?;
    let payload =
        wasm_custom_section(&bytes, "soma.provider").ok_or_else(|| FileProviderLoadError {
            path: path.to_path_buf(),
            message: "WASM provider must contain a `soma.provider` custom section".to_owned(),
        })?;
    serde_json::from_slice(payload).map_err(|source| FileProviderLoadError {
        path: path.to_path_buf(),
        message: format!("invalid WASM provider manifest JSON: {source}"),
    })
}

fn wasm_sidecar_manifest_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        "{}.json",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
    ))
}

fn wasm_custom_section<'a>(bytes: &'a [u8], wanted_name: &str) -> Option<&'a [u8]> {
    if bytes.len() < 8 || &bytes[..4] != b"\0asm" || bytes[4..8] != [1, 0, 0, 0] {
        return None;
    }
    let mut offset = 8;
    while offset < bytes.len() {
        let section_id = *bytes.get(offset)?;
        offset += 1;
        let section_len = read_leb_u32(bytes, &mut offset)? as usize;
        let section_end = offset.checked_add(section_len)?;
        if section_end > bytes.len() {
            return None;
        }
        if section_id == 0 {
            let mut cursor = offset;
            let name_len = read_leb_u32(bytes, &mut cursor)? as usize;
            let name_end = cursor.checked_add(name_len)?;
            if name_end <= section_end && &bytes[cursor..name_end] == wanted_name.as_bytes() {
                return Some(&bytes[name_end..section_end]);
            }
        }
        offset = section_end;
    }
    None
}

fn read_leb_u32(bytes: &[u8], offset: &mut usize) -> Option<u32> {
    let mut result = 0u32;
    let mut shift = 0;
    loop {
        let byte = *bytes.get(*offset)?;
        *offset += 1;
        result |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 32 {
            return None;
        }
    }
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
