//! Global shortcut parsing, registration, rebind, and active-shortcut
//! tracking.
//!
//! This module owns generic `"Modifier+Modifier+Key"` label parsing and the
//! register/unregister mechanics of [`tauri_plugin_global_shortcut`]. It has
//! no opinion on which shortcuts a product allows or defaults to — that
//! policy (e.g. restricting users to a fixed allow-list of labels) belongs to
//! the app.

use std::sync::Mutex;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use crate::command::CommandResult;

/// Tracks the shortcut label currently registered, so callers can unregister
/// only that specific shortcut (rather than every shortcut in the process)
/// when the user rebinds their hotkey.
#[derive(Default)]
pub struct ActiveShortcutState(Mutex<Option<String>>);

impl ActiveShortcutState {
    #[must_use]
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// The currently-registered label, if any and if the lock isn't
    /// poisoned.
    pub fn current(&self) -> Option<String> {
        self.0.lock().ok().and_then(|guard| guard.clone())
    }
}

/// Parse a shortcut label of the form `"Modifier+Modifier+Key"` (e.g.
/// `"Ctrl+Shift+Space"`, `"Alt+F1"`, `"Cmd+K"`) into a [`Shortcut`].
///
/// Recognized modifiers (case-insensitive): `ctrl`/`control`,
/// `alt`/`option`, `shift`, `cmd`/`command`/`super`/`meta`. Recognized keys:
/// single ASCII letters and digits, `space`, `enter`/`return`, `tab`,
/// `escape`/`esc`, arrow keys (`up`/`down`/`left`/`right`), and `f1`-`f12`.
/// Returns `None` for an empty label, a label with no key segment, or an
/// unrecognized modifier/key token.
pub fn parse_shortcut(label: &str) -> Option<Shortcut> {
    let mut segments: Vec<&str> = label
        .split('+')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let key_token = segments.pop()?;
    let code = parse_code(key_token)?;

    let mut modifiers = Modifiers::empty();
    for token in segments {
        modifiers |= parse_modifier(token)?;
    }
    let modifiers = if modifiers.is_empty() {
        None
    } else {
        Some(modifiers)
    };
    Some(Shortcut::new(modifiers, code))
}

fn parse_modifier(token: &str) -> Option<Modifiers> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(Modifiers::CONTROL),
        "alt" | "option" => Some(Modifiers::ALT),
        "shift" => Some(Modifiers::SHIFT),
        "cmd" | "command" | "super" | "meta" => Some(Modifiers::SUPER),
        _ => None,
    }
}

fn parse_code(token: &str) -> Option<Code> {
    let lower = token.to_ascii_lowercase();
    let code = match lower.as_str() {
        "space" => Code::Space,
        "enter" | "return" => Code::Enter,
        "tab" => Code::Tab,
        "escape" | "esc" => Code::Escape,
        "up" => Code::ArrowUp,
        "down" => Code::ArrowDown,
        "left" => Code::ArrowLeft,
        "right" => Code::ArrowRight,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        _ if lower.len() == 1 => single_char_code(lower.chars().next()?)?,
        _ => return None,
    };
    Some(code)
}

fn single_char_code(ch: char) -> Option<Code> {
    match ch {
        'a' => Some(Code::KeyA),
        'b' => Some(Code::KeyB),
        'c' => Some(Code::KeyC),
        'd' => Some(Code::KeyD),
        'e' => Some(Code::KeyE),
        'f' => Some(Code::KeyF),
        'g' => Some(Code::KeyG),
        'h' => Some(Code::KeyH),
        'i' => Some(Code::KeyI),
        'j' => Some(Code::KeyJ),
        'k' => Some(Code::KeyK),
        'l' => Some(Code::KeyL),
        'm' => Some(Code::KeyM),
        'n' => Some(Code::KeyN),
        'o' => Some(Code::KeyO),
        'p' => Some(Code::KeyP),
        'q' => Some(Code::KeyQ),
        'r' => Some(Code::KeyR),
        's' => Some(Code::KeyS),
        't' => Some(Code::KeyT),
        'u' => Some(Code::KeyU),
        'v' => Some(Code::KeyV),
        'w' => Some(Code::KeyW),
        'x' => Some(Code::KeyX),
        'y' => Some(Code::KeyY),
        'z' => Some(Code::KeyZ),
        '0' => Some(Code::Digit0),
        '1' => Some(Code::Digit1),
        '2' => Some(Code::Digit2),
        '3' => Some(Code::Digit3),
        '4' => Some(Code::Digit4),
        '5' => Some(Code::Digit5),
        '6' => Some(Code::Digit6),
        '7' => Some(Code::Digit7),
        '8' => Some(Code::Digit8),
        '9' => Some(Code::Digit9),
        _ => None,
    }
}

/// Register `new_label`'s shortcut, first unregistering whatever shortcut is
/// currently tracked in `state` if it differs. No-ops if `new_label` is
/// already the active shortcut (re-registering an already-registered hotkey
/// errors upstream).
pub fn register_shortcut(
    app: &AppHandle,
    state: &ActiveShortcutState,
    new_label: &str,
) -> CommandResult<()> {
    let new_shortcut = parse_shortcut(new_label)
        .ok_or_else(|| format!("shortcut label `{new_label}` could not be parsed"))?;

    let Ok(mut guard) = state.0.lock() else {
        // Mutex poisoned (some other code path panicked while holding the
        // lock) — fall back to unregister_all for safety. Log at `error`
        // so poisoning has a trail; this clears every global shortcut in
        // the process, not just this app's own, which is worth flagging
        // loudly rather than silently.
        tracing::error!(
            "ActiveShortcutState mutex poisoned; falling back to unregister_all before registering '{new_label}'"
        );
        app.global_shortcut()
            .unregister_all()
            .map_err(|err| err.to_string())?;
        app.global_shortcut()
            .register(new_shortcut)
            .map_err(|err| err.to_string())?;
        return Ok(());
    };

    if guard.as_deref() == Some(new_label) {
        return Ok(());
    }
    if let Some(old_label) = guard.take() {
        if let Some(old_shortcut) = parse_shortcut(&old_label) {
            if let Err(err) = app.global_shortcut().unregister(old_shortcut) {
                tracing::warn!("failed to unregister old shortcut '{old_label}': {err}");
            }
        }
    }
    app.global_shortcut()
        .register(new_shortcut)
        .map_err(|err| err.to_string())?;
    *guard = Some(new_label.to_string());
    Ok(())
}

#[cfg(test)]
#[path = "shortcut_tests.rs"]
mod tests;
