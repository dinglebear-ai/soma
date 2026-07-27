use crate::python_protocol::PYTHON_PROTOCOL_HEADROOM_BYTES;

use super::protocol_output_limit;

#[test]
fn default_python_command_matches_platform_launcher() {
    #[cfg(windows)]
    assert_eq!(super::default_python_command(), "python");

    #[cfg(not(windows))]
    assert_eq!(super::default_python_command(), "python3");
}

#[test]
fn protocol_capture_limit_adds_bounded_headroom() {
    assert_eq!(
        protocol_output_limit(256 * 1024),
        256 * 1024 + PYTHON_PROTOCOL_HEADROOM_BYTES
    );
    assert_eq!(protocol_output_limit(usize::MAX), usize::MAX);
}
