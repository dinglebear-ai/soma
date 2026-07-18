//! Blur-dismiss state and generic window-lifecycle event helpers.
//!
//! A launcher-style window commonly wants two behaviors: hide (rather than
//! quit) when the user clicks the close button, and optionally hide when it
//! loses focus ("click away to dismiss"). This module owns the runtime gate
//! and decision logic; the app wires it into `on_window_event`.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Manager, Window};

/// Runtime gate for blur-dismiss, toggled at runtime (e.g. while a result or
/// settings view is open, the app may want to suppress hide-on-blur so
/// resizing or copying from another window doesn't make it vanish).
pub struct BlurDismissState(AtomicBool);

impl BlurDismissState {
    #[must_use]
    pub fn new(initial: bool) -> Self {
        Self(AtomicBool::new(initial))
    }

    pub fn set(&self, enabled: bool) {
        self.0.store(enabled, Ordering::Relaxed);
    }

    pub fn enabled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

impl Default for BlurDismissState {
    fn default() -> Self {
        Self::new(true)
    }
}

/// Decide whether a focus-lost event should hide the window: both the
/// runtime gate (`blur_dismiss_enabled`, from [`BlurDismissState::enabled`])
/// and the user's persisted preference (`hide_on_blur_pref`, product-owned
/// settings) must allow it.
#[must_use]
pub fn should_hide_on_blur(blur_dismiss_enabled: bool, hide_on_blur_pref: bool) -> bool {
    blur_dismiss_enabled && hide_on_blur_pref
}

/// `WindowEvent::CloseRequested` handler: prevent the actual close and hide
/// the window instead, so the app keeps running in the tray.
pub fn handle_close_requested(window: &Window, api: &tauri::CloseRequestApi) {
    api.prevent_close();
    if let Err(err) = window.hide() {
        tracing::warn!("failed to hide window on close: {err}");
    }
}

/// `WindowEvent::Focused(false)` handler: hide `window` when
/// [`should_hide_on_blur`] allows it. `hide_on_blur_pref` is the caller's
/// product-owned settings value (not this crate's concern).
pub fn handle_focus_lost(window: &Window, state: &BlurDismissState, hide_on_blur_pref: bool) {
    if should_hide_on_blur(state.enabled(), hide_on_blur_pref) {
        if let Err(err) = window.hide() {
            tracing::warn!("failed to hide window on focus loss: {err}");
        }
    }
}

/// Convenience: fetch `state`'s [`BlurDismissState`] from `window`'s
/// `AppHandle` and call [`handle_focus_lost`]. Requires `state` to have been
/// registered via `app.manage(BlurDismissState::default())` (or similar).
pub fn handle_focus_lost_from_managed_state(window: &Window, hide_on_blur_pref: bool) {
    let app = window.app_handle();
    let state = app.state::<BlurDismissState>();
    handle_focus_lost(window, &state, hide_on_blur_pref);
}

#[cfg(test)]
#[path = "blur_tests.rs"]
mod tests;
