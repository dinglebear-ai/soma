use std::time::{Duration, SystemTime};

use super::{stale_binary_warning_for, warning_message};

#[test]
fn no_warning_when_binary_is_newer_than_inputs() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let older = SystemTime::UNIX_EPOCH + Duration::from_secs(50);
    assert_eq!(stale_binary_warning_for(now, [("src", older)]), None);
}

#[test]
fn warning_names_newest_stale_input() {
    let binary = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let src = SystemTime::UNIX_EPOCH + Duration::from_secs(150);
    let cargo = SystemTime::UNIX_EPOCH + Duration::from_secs(125);

    let warning = stale_binary_warning_for(binary, [("Cargo.toml", cargo), ("src", src)])
        .expect("newer source should warn");

    assert!(warning.contains("src"));
    assert!(warning.contains("just install-local"));
}

#[test]
fn warning_message_is_actionable() {
    let message = warning_message("Cargo.toml");
    assert!(message.contains("outdated soma binary"));
    assert!(message.contains("cargo build --release --bin soma"));
}
