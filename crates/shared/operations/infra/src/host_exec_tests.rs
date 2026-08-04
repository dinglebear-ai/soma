use std::path::Path;

use soma_ops::{OperationId, OperationName, Timestamp};

use super::*;

fn deadline() -> Timestamp {
    Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000)
}

#[test]
fn command_allowlist_is_closed() {
    assert_eq!(HostExecCommand::parse("rg").unwrap(), HostExecCommand::Rg);
    assert!(HostExecCommand::parse("bash").is_err());
}

#[test]
fn requests_bound_argv_paths_and_deadlines() {
    let request = HostExecRequest::new(
        OperationId::new(),
        OperationName::new("host.exec").unwrap(),
        HostExecCommand::Ls,
        vec!["-l".into(), "/srv".into()],
        Some("/srv".into()),
        deadline(),
    )
    .unwrap();
    assert_eq!(request.max_stdout_bytes(), 96 * 1024);
    assert_eq!(request.working_dir(), Some(Path::new("/srv")));
    assert!(
        HostExecRequest::new(
            OperationId::new(),
            OperationName::new("host.exec").unwrap(),
            HostExecCommand::Ls,
            vec![String::from("x"); 257],
            None,
            deadline(),
        )
        .is_err()
    );
    assert!(
        HostExecRequest::new(
            OperationId::new(),
            OperationName::new("host.exec").unwrap(),
            HostExecCommand::Ls,
            Vec::new(),
            Some("/srv/../etc".into()),
            deadline(),
        )
        .is_err()
    );
}
