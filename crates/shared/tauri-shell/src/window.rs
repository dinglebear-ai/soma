//! Window show/hide/focus/resize/center/shadow mechanics.
//!
//! These helpers operate on an already-resolved `WebviewWindow`. Resolving
//! the window by label, and deciding *when* to call these (launch, tray
//! click, global shortcut, ...), stays in the app.

use tauri::{LogicalSize, Size, WebviewWindow};

use crate::command::{CommandResult, TauriResultExt};

/// Hide the window without closing it.
pub fn hide(window: &WebviewWindow) -> CommandResult<()> {
    window.hide().command_result()
}

/// Show, un-maximize (if needed), focus, and raise `window`.
pub fn show_and_focus(window: &WebviewWindow) -> CommandResult<()> {
    if window.is_maximized().unwrap_or(false) {
        if let Err(err) = window.unmaximize() {
            tracing::warn!("failed to unmaximize window before showing: {err}");
        }
    }
    window.show().command_result()?;
    window.set_focus().command_result()
}

/// Resize `window` to `(width, height)` logical pixels, toggle its native
/// shadow, and re-center it. A maximized window ignores `set_size` on
/// Windows, so maximize is dropped first.
pub fn resize_and_center(
    window: &WebviewWindow,
    width: f64,
    height: f64,
    shadow: bool,
) -> CommandResult<()> {
    if window.is_maximized().unwrap_or(false) {
        if let Err(err) = window.unmaximize() {
            tracing::warn!("failed to unmaximize window before resizing: {err}");
        }
    }
    window
        .set_size(Size::Logical(LogicalSize { width, height }))
        .command_result()?;
    if let Err(err) = window.set_shadow(shadow) {
        tracing::warn!("failed to set window shadow: {err}");
    }
    window.center().command_result()
}

/// Flip `window` between maximized and restored.
pub fn toggle_maximize(window: &WebviewWindow) -> CommandResult<()> {
    if window.is_maximized().command_result()? {
        window.unmaximize().command_result()
    } else {
        window.maximize().command_result()
    }
}

/// Best-effort visibility check; treats an error resolving visibility as
/// "not visible" so callers default to showing the window.
pub fn is_visible(window: &WebviewWindow) -> bool {
    window.is_visible().unwrap_or_else(|err| {
        tracing::warn!("failed to query window visibility, assuming hidden: {err}");
        false
    })
}

// No unit tests here: every function requires a live `WebviewWindow`, which
// needs a running Tauri application to construct. These are thin,
// directly-inspectable wrappers over `tauri::WebviewWindow` methods; the
// app's own manual/smoke testing (see `apps/palette/CLAUDE.md`) is the
// verification surface for actual window behavior, matching how
// `apps/palette/src-tauri` tested this code before extraction.
