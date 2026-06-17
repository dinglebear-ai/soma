use super::*;

#[test]
fn local_and_stdio_modes_default_to_quiet_logging() {
    assert_eq!(default_log_level(true, false), "warn");
    assert_eq!(default_log_level(false, false), "warn");
}

#[test]
fn http_server_mode_defaults_to_info_logging() {
    assert_eq!(default_log_level(false, true), "info");
}
