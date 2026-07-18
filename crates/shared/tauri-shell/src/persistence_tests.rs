use std::collections::BTreeMap;

use super::{atomic_write, env_var_or_none, parse_json, read_json_or_default, write_json_atomic};

#[test]
fn atomic_write_then_read_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    atomic_write(&path, b"{\"a\":1}").unwrap();
    let contents = std::fs::read_to_string(&path).unwrap();
    assert_eq!(contents, "{\"a\":1}");
}

#[test]
fn atomic_write_leaves_no_temp_file_behind() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    write_json_atomic(&path, &BTreeMap::from([("k", "v")])).unwrap();
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![std::ffi::OsString::from("settings.json")]);
}

#[test]
fn write_json_atomic_creates_parent_directories() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("deep").join("settings.json");
    write_json_atomic(&path, &42u32).unwrap();
    let value: u32 = parse_json(&std::fs::read_to_string(&path).unwrap(), &path).unwrap();
    assert_eq!(value, 42);
}

#[derive(Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
struct Prefs {
    #[serde(default)]
    theme: Option<String>,
}

#[test]
fn read_json_or_default_returns_default_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("does-not-exist.json");
    let prefs: Prefs = read_json_or_default(&path).unwrap();
    assert_eq!(prefs, Prefs::default());
}

#[test]
fn read_json_or_default_parses_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prefs.json");
    write_json_atomic(
        &path,
        &Prefs {
            theme: Some("dark".to_string()),
        },
    )
    .unwrap();
    let prefs: Prefs = read_json_or_default(&path).unwrap();
    assert_eq!(prefs.theme.as_deref(), Some("dark"));
}

#[test]
fn read_json_or_default_surfaces_parse_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.json");
    std::fs::write(&path, b"not json").unwrap();
    let result: Result<Prefs, String> = read_json_or_default(&path);
    assert!(result.is_err());
}

#[test]
fn env_var_or_none_treats_blank_as_unset() {
    let key = "SOMA_TAURI_SHELL_TEST_ENV_VAR_BLANK";
    // SAFETY: test-only env mutation scoped to a unique key.
    unsafe {
        std::env::set_var(key, "   ");
    }
    assert_eq!(env_var_or_none(key), None);
    unsafe {
        std::env::remove_var(key);
    }
}

#[test]
fn env_var_or_none_returns_untrimmed_set_value() {
    let key = "SOMA_TAURI_SHELL_TEST_ENV_VAR_SET";
    // SAFETY: test-only env mutation scoped to a unique key.
    unsafe {
        std::env::set_var(key, " value ");
    }
    assert_eq!(env_var_or_none(key), Some(" value ".to_string()));
    unsafe {
        std::env::remove_var(key);
    }
}
