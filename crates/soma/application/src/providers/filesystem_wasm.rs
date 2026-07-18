//! WASM provider manifest parsing: the `soma.provider` custom-section reader
//! and the `.wasm.json` sidecar manifest convention. Split out of
//! `filesystem.rs` to stay under the module size hard limit — pre-existing
//! logic, unchanged, just relocated.

use std::{fs, path::Path, path::PathBuf};

use serde_json::Value;

use super::FileProviderLoadError;

pub(super) fn load_wasm_catalog_value(path: &Path) -> Result<Value, FileProviderLoadError> {
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

pub(super) fn wasm_sidecar_manifest_path(path: &Path) -> PathBuf {
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

#[cfg(test)]
#[path = "filesystem_wasm_tests.rs"]
mod tests;
