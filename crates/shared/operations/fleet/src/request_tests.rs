use std::path::Path;

use soma_ops::Timestamp;

use super::*;
use crate::{CommandOutput, CommandRequest, HostId, TransferReceipt, TransferRequest};

fn deadline() -> Timestamp {
    Timestamp::from_unix_millis(10_000)
}

#[test]
fn command_requests_preserve_exec_style_arguments() {
    let request = CommandRequest::new("printf", ["%s", "hello; rm -rf /"], deadline())
        .unwrap()
        .with_working_dir("/tmp")
        .unwrap()
        .with_output_limits(1024, 2048)
        .unwrap();
    assert_eq!(request.program(), "printf");
    assert_eq!(request.args(), &["%s", "hello; rm -rf /"]);
    assert_eq!(request.working_dir(), Some(Path::new("/tmp")));
    assert_eq!(request.max_stdout_bytes(), 1024);
    assert_eq!(request.max_stderr_bytes(), 2048);
    request
        .validate_at(Timestamp::from_unix_millis(9_999))
        .unwrap();
}

#[test]
fn command_requests_reject_invalid_bounds() {
    assert!(CommandRequest::new("", Vec::<String>::new(), deadline()).is_err());
    assert!(
        CommandRequest::new("echo", (0..257).map(|index| index.to_string()), deadline()).is_err()
    );
    assert!(CommandRequest::new("echo", ["hello\0world"], deadline()).is_err());
    assert!(
        CommandRequest::new("echo", ["hello"], deadline())
            .unwrap()
            .with_working_dir("../tmp")
            .is_err()
    );
    assert!(
        CommandRequest::new("echo", ["hello"], deadline())
            .unwrap()
            .with_output_limits(0, 1)
            .is_err()
    );
    assert_eq!(
        CommandRequest::new("echo", ["hello"], deadline())
            .unwrap()
            .validate_at(deadline()),
        Err(RequestError::DeadlineElapsed)
    );
}

#[test]
fn transfer_requests_are_absolute_bounded_and_deadline_aware() {
    let request = TransferRequest::new(
        HostId::new("devhost").unwrap(),
        "/tmp/source",
        HostId::new("edgehost").unwrap(),
        "/tmp/destination",
        4096,
        deadline(),
    )
    .unwrap();
    assert_eq!(request.source_path(), Path::new("/tmp/source"));
    assert_eq!(request.destination_path(), Path::new("/tmp/destination"));
    assert_eq!(request.max_bytes(), 4096);
    request.validate_at(Timestamp::from_unix_millis(1)).unwrap();

    assert!(
        TransferRequest::new(
            HostId::new("devhost").unwrap(),
            "relative",
            HostId::new("edgehost").unwrap(),
            "/tmp/destination",
            4096,
            deadline(),
        )
        .is_err()
    );
    assert!(
        TransferRequest::new(
            HostId::new("devhost").unwrap(),
            "/tmp/source",
            HostId::new("edgehost").unwrap(),
            "/tmp/destination",
            0,
            deadline(),
        )
        .is_err()
    );
}

#[test]
fn transfer_receipts_verify_matching_digests() {
    let receipt = TransferReceipt::new(42)
        .with_digests("a".repeat(64), "a".repeat(64))
        .unwrap();
    assert_eq!(receipt.bytes(), 42);
    assert!(receipt.verified());
    assert!(
        TransferReceipt::new(42)
            .with_digests("not-a-digest", "a".repeat(64))
            .is_err()
    );
}

#[test]
fn command_output_preserves_bytes_and_completion_state() {
    let output = CommandOutput::new(b"ok".to_vec(), b"warn".to_vec(), Some(0), true);
    assert_eq!(output.stdout(), b"ok");
    assert_eq!(output.stderr(), b"warn");
    assert_eq!(output.exit_code(), Some(0));
    assert!(output.truncated());
}
