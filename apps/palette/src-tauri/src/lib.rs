use std::fmt::Display;

use serde::{Deserialize, Serialize};
use soma_tauri_shell::{app, blur::BlurDismissState, shortcut::ActiveShortcutState, tray, window};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

mod labby_bridge;
mod oauth;
mod persistence;
mod window_events;

use labby_bridge::BridgeClient;
use persistence::*;

/// Log a warning through `tracing`. Replaces the former Axon `diag` wrapper; see
/// `docs/dev/OBSERVABILITY.md` — use `tracing`, never a custom logger.
pub(crate) fn warn(message: impl Display) {
    tracing::warn!("{message}");
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LabbySettings {
    server_url: String,
    static_token: Option<String>,
    shortcut: String,
    theme: PaletteTheme,
    hide_on_blur: bool,
    open_results_inline: bool,
    show_footer_hints: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum PaletteTheme {
    System,
    Dark,
    Light,
}

const DEFAULT_SERVER_URL: &str = "http://localhost:8765";
const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+Space";
const SETTINGS_FILE: &str = "settings.json";

fn log_palette_warning(context: &str, err: impl Display) {
    warn(format!("{context}: {err}"));
}

#[tauri::command]
fn load_palette_config(app: AppHandle) -> Result<LabbySettings, String> {
    merged_settings(&app)
}

#[tauri::command]
fn load_palette_default_config() -> LabbySettings {
    default_settings()
}

#[tauri::command]
fn save_palette_settings(app: AppHandle, settings: LabbySettings) -> Result<LabbySettings, String> {
    let settings = normalize_settings(settings);
    // 1. Persist palette-only preferences.
    save_palette_prefs(&app, &settings)?;
    // 2. Only mutate runtime state (shortcut) after the write succeeds.
    update_shortcut(&app, &settings)?;
    Ok(settings)
}

fn save_palette_prefs(app: &AppHandle, settings: &LabbySettings) -> Result<(), String> {
    write_settings(app, settings).map_err(|err| err.to_string())
}

fn update_shortcut(app: &AppHandle, settings: &LabbySettings) -> Result<(), String> {
    register_configured_shortcut(app, settings)
}

#[tauri::command]
fn hide_palette(app: AppHandle) -> Result<(), String> {
    window::hide(&app::get_window(&app, "main")?)
}

#[tauri::command]
fn show_palette(app: AppHandle) -> Result<(), String> {
    show_main_window(&app)
}

#[tauri::command]
fn resize_palette(app: AppHandle, width: f64, height: f64, shadow: bool) -> Result<(), String> {
    // A maximized window ignores set_size on Windows; `resize_and_center`
    // drops maximize first so the auto-sizer (and the next launcher open)
    // always lands at the intended size.
    window::resize_and_center(&app::get_window(&app, "main")?, width, height, shadow)
}

#[tauri::command]
fn toggle_maximize(app: AppHandle) -> Result<(), String> {
    window::toggle_maximize(&app::get_window(&app, "main")?)
}

#[tauri::command]
fn set_blur_dismiss(state: tauri::State<'_, BlurDismissState>, enabled: bool) {
    state.set(enabled);
}

fn merged_settings(app: &AppHandle) -> Result<LabbySettings, String> {
    let persisted = read_settings_result(app)?;
    let defaults = default_settings();
    Ok(merge_settings(persisted, defaults))
}

fn merged_settings_or_default(app: &AppHandle) -> LabbySettings {
    match merged_settings(app) {
        Ok(settings) => settings,
        Err(err) => {
            warn(&err);
            default_settings()
        }
    }
}

fn merge_settings(persisted: PartialPaletteSettings, defaults: LabbySettings) -> LabbySettings {
    normalize_settings(LabbySettings {
        server_url: persisted.server_url.unwrap_or(defaults.server_url),
        static_token: persisted.static_token.unwrap_or(defaults.static_token),
        shortcut: persisted
            .shortcut
            .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string()),
        theme: persisted.theme.unwrap_or(PaletteTheme::System),
        hide_on_blur: persisted.hide_on_blur.unwrap_or(true),
        open_results_inline: persisted.open_results_inline.unwrap_or(true),
        show_footer_hints: persisted.show_footer_hints.unwrap_or(false),
    })
}

fn default_settings() -> LabbySettings {
    let server_url = default_server_url(
        soma_tauri_shell::persistence::env_var_or_none("LABBY_API_URL").as_deref(),
        soma_tauri_shell::persistence::env_var_or_none("LABBY_PUBLIC_URL").as_deref(),
    );
    let static_token = soma_tauri_shell::persistence::env_var_or_none("LABBY_MCP_HTTP_TOKEN")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    LabbySettings {
        server_url,
        static_token,
        shortcut: DEFAULT_SHORTCUT.to_string(),
        theme: PaletteTheme::System,
        hide_on_blur: true,
        open_results_inline: true,
        show_footer_hints: false,
    }
}

fn default_server_url(api_url: Option<&str>, public_url: Option<&str>) -> String {
    api_url
        .or(public_url)
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string())
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PartialPaletteSettings {
    server_url: Option<String>,
    static_token: Option<Option<String>>,
    shortcut: Option<String>,
    theme: Option<PaletteTheme>,
    hide_on_blur: Option<bool>,
    open_results_inline: Option<bool>,
    show_footer_hints: Option<bool>,
}

fn normalize_settings(mut settings: LabbySettings) -> LabbySettings {
    settings.server_url = normalize_server_url(&settings.server_url);
    if settings.server_url.is_empty() {
        settings.server_url = DEFAULT_SERVER_URL.to_string();
    }
    settings.static_token = settings
        .static_token
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty());
    settings.shortcut = normalize_shortcut_label(&settings.shortcut);
    settings
}

/// Normalise a user-entered server URL down to its origin (scheme + host +
/// port), silently dropping any path/query/fragment.
///
/// Labby exposes multiple surfaces at the same host — `/mcp` (MCP transport),
/// `/v1/*` (this app's REST API), `/authorize` (OAuth) — so it's an easy
/// mistake to paste the MCP URL (e.g. `https://labby.example.com/mcp`) into
/// this field. Silently stripping the path rather than hard-erroring means
/// that mistake just works instead of breaking OAuth status/login with an
/// opaque "invalid server URL" failure.
fn normalize_server_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else if trimmed.starts_with("localhost") || trimmed.starts_with("127.0.0.1") {
        format!("http://{trimmed}")
    } else {
        format!("https://{trimmed}")
    };
    match reqwest::Url::parse(&with_scheme) {
        Ok(url) if url.host_str().is_some() => url.origin().ascii_serialization(),
        _ => with_scheme,
    }
}

fn normalize_shortcut_label(shortcut: &str) -> String {
    match shortcut.trim().to_ascii_lowercase().as_str() {
        "alt+space" | "option+space" => "Alt+Space".to_string(),
        "ctrl+space" | "control+space" => "Ctrl+Space".to_string(),
        "cmd+shift+space" | "command+shift+space" | "super+shift+space" => {
            "Cmd+Shift+Space".to_string()
        }
        _ => DEFAULT_SHORTCUT.to_string(),
    }
}

/// Validate a saved Labby server URL. `normalize_server_url` already strips
/// any path/query/fragment down to the origin, so this only needs to reject
/// an empty/unparsable value or a non-http(s) scheme.
pub(crate) fn validate_saved_server_url(server_url: &str) -> Result<String, String> {
    let server_url = normalize_server_url(server_url);
    if server_url.is_empty() {
        return Err("no Labby server URL is configured — set one in Settings".to_string());
    }
    let parsed = reqwest::Url::parse(&server_url)
        .map_err(|err| format!("saved Labby server URL is invalid: {err}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("saved Labby server URL must use http or https".to_string());
    }
    if parsed.host_str().is_none() {
        return Err("saved Labby server URL must include a host".to_string());
    }
    Ok(server_url)
}

/// Palette only allows rebinding to one of a small fixed set of shortcuts
/// (`normalize_shortcut_label` above) — that allow-list is product policy
/// owned by this app. `soma_tauri_shell::shortcut` owns the generic
/// label-parsing and register/unregister mechanics for whatever label it's
/// given.
fn register_configured_shortcut(app: &AppHandle, settings: &LabbySettings) -> Result<(), String> {
    let new_label = normalize_shortcut_label(&settings.shortcut);
    let state = app.state::<ActiveShortcutState>();
    soma_tauri_shell::shortcut::register_shortcut(app, &state, &new_label)
}

fn show_main_window(app: &AppHandle) -> Result<(), String> {
    let webview = app::get_window(app, "main")?;
    // Compact launcher — matches COMPACT in useWindowChrome.ts (bar + inset).
    // Compact floats a CSS-glowing bar; keep the native shadow off (JS
    // re-asserts it).
    window::resize_and_center(&webview, 720.0, 92.0, false)?;
    window::show_and_focus(&webview)?;
    app::emit_or_warn(&webview, "palette://shown", ());
    Ok(())
}

fn toggle_main_window(app: &AppHandle) {
    let Ok(webview) = app::get_window(app, "main") else {
        return;
    };
    if window::is_visible(&webview) {
        if let Err(err) = window::hide(&webview) {
            log_palette_warning("failed to hide main window", err);
        }
    } else if let Err(err) = show_main_window(app) {
        log_palette_warning("failed to show main window", err);
    }
}

fn install_tray(app: &tauri::App) -> tauri::Result<()> {
    let items = [
        tray::TrayMenuItemSpec::new("show", "Show Palette"),
        tray::TrayMenuItemSpec::new("settings", "Settings"),
        tray::TrayMenuItemSpec::new("quit", "Quit Labby Palette"),
    ];
    let menu = tray::build_tray_menu(app, &items)?;

    tray::install_tray(
        app,
        "Labby Palette",
        &menu,
        |app, id| match id {
            "show" => {
                if let Err(err) = show_main_window(app) {
                    log_palette_warning("failed to show main window from tray", err);
                }
            }
            "settings" => {
                if let Err(err) = show_main_window(app) {
                    log_palette_warning("failed to show main window for settings", err);
                }
                match app::get_window(app, "main") {
                    Ok(webview) => app::emit_or_warn(&webview, "palette://open-settings", ()),
                    Err(err) => log_palette_warning("failed to open settings", err),
                }
            }
            "quit" => {
                if let Err(err) = app.global_shortcut().unregister_all() {
                    log_palette_warning("failed to unregister global shortcuts on quit", err);
                }
                app.exit(0);
            }
            _ => {}
        },
        toggle_main_window,
    )
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let bridge_client = BridgeClient::new()
        .map_err(|err| format!("failed to build HTTP client for Labby bridge: {err}"))?;

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_main_window(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            load_palette_config,
            load_palette_default_config,
            save_palette_settings,
            hide_palette,
            show_palette,
            resize_palette,
            toggle_maximize,
            set_blur_dismiss,
            labby_bridge::fetch_catalog,
            labby_bridge::dispatch_action,
            labby_bridge::fetch_launcher_catalog,
            labby_bridge::fetch_launcher_schema,
            labby_bridge::execute_launcher_entry,
            oauth::labby_oauth_login,
            oauth::labby_oauth_logout,
            oauth::labby_oauth_status
        ])
        .manage(BlurDismissState::new(true))
        .manage(ActiveShortcutState::new())
        .manage(bridge_client)
        .manage(oauth::OauthState::new())
        .setup(|app| {
            if let Err(err) = install_tray(app) {
                log_palette_warning("failed to install tray icon", err);
            }
            let settings = merged_settings_or_default(app.handle());
            register_configured_shortcut(app.handle(), &settings).map_err(anyhow::Error::msg)?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let window_handle = handle.clone();
                if let Err(err) = handle.run_on_main_thread(move || {
                    if let Err(err) = show_main_window(&window_handle) {
                        log_palette_warning("failed to show main window on launch", err);
                    }
                }) {
                    log_palette_warning("failed to schedule launch window show", err);
                }
            });
            Ok(())
        })
        .on_window_event(window_events::handle_window_event)
        .run(tauri::generate_context!())
        .map_err(|err| format!("error while running Labby Palette: {err}").into())
}

#[cfg(test)]
mod tests {
    use super::default_server_url;

    #[test]
    fn default_server_url_prefers_dedicated_api_url() {
        assert_eq!(
            default_server_url(
                Some(" http://127.0.0.1:8765/ "),
                Some("https://labby.example.com")
            ),
            "http://127.0.0.1:8765"
        );
    }

    #[test]
    fn default_server_url_falls_back_to_public_url_for_compatibility() {
        assert_eq!(
            default_server_url(None, Some("https://labby.example.com/")),
            "https://labby.example.com"
        );
    }
}
