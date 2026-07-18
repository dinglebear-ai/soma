use super::{parse_code, parse_modifier, parse_shortcut, ActiveShortcutState};
use tauri_plugin_global_shortcut::{Code, Modifiers};

#[test]
fn parses_single_modifier_and_key() {
    let shortcut = parse_shortcut("Ctrl+Space").expect("valid shortcut");
    assert_eq!(
        shortcut,
        tauri_plugin_global_shortcut::Shortcut::new(Some(Modifiers::CONTROL), Code::Space)
    );
}

#[test]
fn parses_multiple_modifiers_in_any_order() {
    let a = parse_shortcut("Ctrl+Shift+Space").expect("valid shortcut");
    let b = parse_shortcut("Shift+Ctrl+Space").expect("valid shortcut");
    assert_eq!(a, b);
}

#[test]
fn parses_key_only_shortcut_with_no_modifier() {
    let shortcut = parse_shortcut("F1").expect("valid shortcut");
    assert_eq!(
        shortcut,
        tauri_plugin_global_shortcut::Shortcut::new(None, Code::F1)
    );
}

#[test]
fn is_case_and_whitespace_insensitive() {
    let a = parse_shortcut(" ctrl + shift + space ").expect("valid shortcut");
    let b = parse_shortcut("CTRL+SHIFT+SPACE").expect("valid shortcut");
    assert_eq!(a, b);
}

#[test]
fn parses_letter_and_digit_keys() {
    assert!(parse_shortcut("Cmd+K").is_some());
    assert!(parse_shortcut("Alt+1").is_some());
}

#[test]
fn rejects_empty_label() {
    assert!(parse_shortcut("").is_none());
    assert!(parse_shortcut("   ").is_none());
}

#[test]
fn rejects_unknown_modifier() {
    assert!(parse_shortcut("Hyper+Space").is_none());
}

#[test]
fn rejects_unknown_key() {
    assert!(parse_shortcut("Ctrl+Banana").is_none());
}

#[test]
fn accepts_modifier_aliases() {
    assert_eq!(parse_modifier("control"), Some(Modifiers::CONTROL));
    assert_eq!(parse_modifier("option"), Some(Modifiers::ALT));
    assert_eq!(parse_modifier("command"), Some(Modifiers::SUPER));
    assert_eq!(parse_modifier("meta"), Some(Modifiers::SUPER));
}

#[test]
fn accepts_arrow_and_editing_keys() {
    assert_eq!(parse_code("up"), Some(Code::ArrowUp));
    assert_eq!(parse_code("Return"), Some(Code::Enter));
    assert_eq!(parse_code("esc"), Some(Code::Escape));
}

#[test]
fn active_shortcut_state_starts_empty() {
    let state = ActiveShortcutState::new();
    assert_eq!(state.current(), None);
}
