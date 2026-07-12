use std::{
    fs,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use serde_json::json;
use soma_contracts::providers::{ProviderCatalog, ProviderKind};

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

impl FileProviderSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn load(&self) -> Result<Vec<std::sync::Arc<dyn Provider>>, FileProviderLoadError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut providers = Vec::new();
        let entries = fs::read_dir(&self.root).map_err(|source| FileProviderLoadError {
            path: self.root.clone(),
            message: format!("failed to read provider directory: {source}"),
        })?;

        for entry in entries {
            let entry = entry.map_err(|source| FileProviderLoadError {
                path: self.root.clone(),
                message: format!("failed to read provider directory entry: {source}"),
            })?;
            let path = entry.path();
            if !path.is_file() || !is_provider_file(&path) {
                continue;
            }
            let catalog = load_catalog(&path)?;
            if catalog.provider.enabled == Some(false) {
                continue;
            }
            providers.push(provider_for_catalog(path, catalog));
        }
        Ok(providers)
    }
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
