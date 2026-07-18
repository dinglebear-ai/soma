//! Tray icon setup helpers.
//!
//! Builds a tray icon with a caller-supplied menu and click behavior. This
//! module owns the `tauri::tray` wiring boilerplate; the app supplies menu
//! item labels/ids and what each event should do.

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Wry,
};

/// One entry in a tray context menu.
pub struct TrayMenuItemSpec {
    pub id: &'static str,
    pub label: String,
    pub enabled: bool,
}

impl TrayMenuItemSpec {
    #[must_use]
    pub fn new(id: &'static str, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            enabled: true,
        }
    }
}

/// Build a `tauri::menu::Menu` from a flat list of item specs.
pub fn build_tray_menu(app: &App, items: &[TrayMenuItemSpec]) -> tauri::Result<Menu<Wry>> {
    let mut menu_items = Vec::with_capacity(items.len());
    for item in items {
        menu_items.push(MenuItem::with_id(
            app,
            item.id,
            &item.label,
            item.enabled,
            None::<&str>,
        )?);
    }
    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = menu_items
        .iter()
        .map(|item| item as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    Menu::with_items(app, &refs)
}

/// Install a tray icon using the app's default window icon, with `tooltip`,
/// `menu`, and the two behaviors every launcher-style tray needs:
/// `on_menu_item` fires with the clicked item's id; `on_left_click` fires on
/// a tray-icon left click (mouse up).
pub fn install_tray(
    app: &App,
    tooltip: &str,
    menu: &Menu<Wry>,
    on_menu_item: impl Fn(&tauri::AppHandle, &str) + Send + Sync + 'static,
    on_left_click: impl Fn(&tauri::AppHandle) + Send + Sync + 'static,
) -> tauri::Result<()> {
    let icon = app.default_window_icon().cloned();
    let mut tray = TrayIconBuilder::new()
        .tooltip(tooltip)
        .menu(menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| on_menu_item(app, event.id().as_ref()))
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                on_left_click(tray.app_handle());
            }
        });

    if let Some(icon) = icon {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}
