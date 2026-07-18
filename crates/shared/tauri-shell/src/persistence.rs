//! Generic app-data path resolution and atomic JSON persistence helpers.
//!
//! This module owns *mechanics* only: given a path and a serializable value,
//! write it atomically; given a path, read and parse a JSON value. It has no
//! knowledge of any particular product's settings shape or file name — the
//! caller supplies both.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{de::DeserializeOwned, Serialize};
use tauri::{AppHandle, Manager};

use crate::command::CommandResult;

/// Resolve `file_name` inside the app's config directory (e.g.
/// `~/.config/<app>/settings.json` on Linux). Does not create the directory;
/// callers that write should create parents first (see [`write_json_atomic`]).
pub fn app_data_path(app: &AppHandle, file_name: &str) -> CommandResult<PathBuf> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(file_name))
        .map_err(|err| format!("failed to resolve app config directory: {err}"))
}

/// Read and parse `path` as JSON. A missing file is not an error — it
/// returns `T::default()`, matching the common "no settings saved yet" case.
pub fn read_json_or_default<T: DeserializeOwned + Default>(path: &Path) -> CommandResult<T> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(T::default()),
        Err(err) => {
            return Err(format!("failed to read {}: {err}", path.display()));
        }
    };
    parse_json(&contents, path)
}

/// Parse `contents` as JSON, producing a message that names `path` on
/// failure. Split out from [`read_json_or_default`] so parse failures can be
/// tested without touching the filesystem.
pub fn parse_json<T: DeserializeOwned>(contents: &str, path: &Path) -> CommandResult<T> {
    serde_json::from_str(contents)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))
}

/// Serialize `value` to pretty JSON and write it to `path` atomically,
/// creating parent directories as needed.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> CommandResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create directory {}: {err}", parent.display()))?;
    }
    let body =
        serde_json::to_string_pretty(value).map_err(|err| format!("failed to serialize: {err}"))?;
    atomic_write(path, body.as_bytes())
}

/// Write `data` to `path` atomically: write to a per-write unique temp file,
/// fsync, then rename onto the target.
///
/// The temp name carries a UUID so two concurrent writers of the same `path`
/// do not collide on a fixed `<path>.tmp`. If any step fails the temp file is
/// best-effort removed so unique temps don't accumulate on error.
///
/// On Unix, the temp file is created with mode `0o600` atomically via
/// `OpenOptions::mode`, so it is never world-readable even momentarily. On
/// Windows no explicit permission change is applied; rely on the directory
/// ACL to restrict access.
pub fn atomic_write(path: &Path, data: &[u8]) -> CommandResult<()> {
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let write = || -> CommandResult<()> {
        {
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);

            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }

            let mut file = opts
                .open(&tmp)
                .map_err(|err| format!("failed to open {}: {err}", tmp.display()))?;

            use std::io::Write;
            file.write_all(data)
                .map_err(|err| format!("failed to write {}: {err}", tmp.display()))?;
            file.sync_all()
                .map_err(|err| format!("failed to sync {}: {err}", tmp.display()))?;
        }
        fs::rename(&tmp, path)
            .map_err(|err| format!("failed to rename into {}: {err}", path.display()))
    };
    write().inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Read an environment variable, returning `None` for a missing or
/// whitespace-only value. The returned value is *not* trimmed — callers that
/// need a trimmed value should trim it themselves, matching how this
/// distinguishes "unset" from "set to something".
pub fn env_var_or_none(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
#[path = "persistence_tests.rs"]
mod tests;
