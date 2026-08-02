use soma_ops::Timestamp;

use super::*;

#[test]
fn command_contract_defaults_are_bounded() {
    let request = CommandRequest::new(
        "hostname",
        Vec::<String>::new(),
        Timestamp::from_unix_millis(100),
    )
    .unwrap();
    assert_eq!(request.max_stdout_bytes(), 256 * 1024);
    assert_eq!(request.max_stderr_bytes(), 256 * 1024);
    assert!(request.working_dir().is_none());
}
