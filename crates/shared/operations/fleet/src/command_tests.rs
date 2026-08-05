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

#[test]
fn command_transport_allows_bounded_typed_launcher_overhead() {
    let accepted = vec!["x"; 320];
    assert!(CommandRequest::new("python3", accepted, Timestamp::from_unix_millis(100),).is_ok());
    let rejected = vec!["x"; 321];
    assert!(matches!(
        CommandRequest::new("python3", rejected, Timestamp::from_unix_millis(100)),
        Err(RequestError::TooManyArguments {
            count: 321,
            max: 320
        })
    ));
}

#[test]
fn command_stdin_is_optional_and_bounded() {
    let request = CommandRequest::new(
        "cat",
        Vec::<String>::new(),
        Timestamp::from_unix_millis(100),
    )
    .unwrap()
    .with_stdin(b"hello".to_vec())
    .unwrap();
    assert_eq!(request.stdin(), Some(b"hello".as_slice()));
    assert!(
        CommandRequest::new(
            "cat",
            Vec::<String>::new(),
            Timestamp::from_unix_millis(100),
        )
        .unwrap()
        .with_stdin(vec![0; 64 * 1024 * 1024 + 1])
        .is_err()
    );
}
