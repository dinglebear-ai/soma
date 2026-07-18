use soma_tauri_shell::blur::{handle_close_requested, handle_focus_lost, BlurDismissState};
use tauri::{Manager, Window, WindowEvent};

use super::merged_settings_or_default;

pub(super) fn handle_window_event(window: &Window, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } => handle_close_requested(window, api),
        WindowEvent::Focused(false) => {
            let app = window.app_handle();
            let state = app.state::<BlurDismissState>();
            let hide_on_blur_pref = merged_settings_or_default(app).hide_on_blur;
            handle_focus_lost(window, &state, hide_on_blur_pref);
        }
        _ => {}
    }
}
