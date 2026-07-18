//! `AppHandle`-level helpers: resolving a window by label, toggling its
//! visibility, and emitting a frontend event without letting the failure
//! propagate as a command error.

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::{command::CommandResult, window};

/// Resolve `label`'s webview window, or a descriptive error if it isn't
/// open.
pub fn get_window(app: &AppHandle, label: &str) -> CommandResult<WebviewWindow> {
    app.get_webview_window(label)
        .ok_or_else(|| format!("{label} window not found"))
}

/// Show `label`'s window if hidden, hide it if visible.
pub fn toggle_window_visibility(app: &AppHandle, label: &str) -> CommandResult<()> {
    let Some(handle) = app.get_webview_window(label) else {
        return Ok(());
    };
    if window::is_visible(&handle) {
        window::hide(&handle)
    } else {
        window::show_and_focus(&handle)
    }
}

/// Emit `event` with `payload` on `window`, logging (rather than
/// propagating) a failure. Frontend event delivery is best-effort — a failed
/// emit shouldn't fail the command that triggered it.
pub fn emit_or_warn<S: serde::Serialize + Clone>(window: &WebviewWindow, event: &str, payload: S) {
    if let Err(err) = window.emit(event, payload) {
        tracing::warn!("failed to emit {event}: {err}");
    }
}
