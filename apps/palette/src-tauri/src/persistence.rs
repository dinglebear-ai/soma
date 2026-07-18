//! Disk persistence for the palette: reads/writes the palette preferences file
//! (`settings.json`) beside the OAuth credential file in the app config dir.
//!
//! The palette does not manage a `labby serve` instance's `~/.labby/.env` or
//! `config.toml` — that is owned by `labby setup`. This module only owns the
//! product-specific shape (`LabbySettings`, `PartialPaletteSettings`) and file
//! name; the generic app-data path resolution and atomic-JSON-write mechanics
//! (including the `settings.json`/`oauth.json` atomic-write pattern) live in
//! `soma_tauri_shell::persistence`, re-exported here as [`atomic_write`] for
//! `oauth::store`.

use std::path::Path;

use tauri::AppHandle;

use crate::{LabbySettings, PartialPaletteSettings, SETTINGS_FILE};

pub(crate) fn read_settings_result(app: &AppHandle) -> Result<PartialPaletteSettings, String> {
    let path = match soma_tauri_shell::persistence::app_data_path(app, SETTINGS_FILE) {
        Ok(path) => path,
        Err(err) => {
            crate::warn(err);
            return Ok(PartialPaletteSettings::default());
        }
    };
    soma_tauri_shell::persistence::read_json_or_default(&path)
}

pub(crate) fn write_settings(
    app: &AppHandle,
    settings: &LabbySettings,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = soma_tauri_shell::persistence::app_data_path(app, SETTINGS_FILE)?;
    soma_tauri_shell::persistence::write_json_atomic(&path, settings)?;
    Ok(())
}

/// Re-exported for `oauth::store`, which persists `oauth.json` beside
/// `settings.json` using the same atomic-write pattern.
pub(crate) fn atomic_write(path: &Path, data: &[u8]) -> Result<(), String> {
    soma_tauri_shell::persistence::atomic_write(path, data)
}
