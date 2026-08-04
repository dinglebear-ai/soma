use std::path::Path;

use soma_ops::{OperationId, OperationName, Timestamp};

use super::*;

fn deadline() -> Timestamp {
    Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000)
}

#[test]
fn container_exec_requests_are_direct_bounded_and_non_tty() {
    let request = ContainerExecRequest::new(
        OperationId::new(),
        OperationName::new("container.exec").unwrap(),
        "api",
        vec!["printf".into(), "hello".into()],
        Some("1000".into()),
        Some("/app".into()),
        deadline(),
    )
    .unwrap();
    assert_eq!(request.command()[0], "printf");
    assert_eq!(request.user(), Some("1000"));
    assert_eq!(request.working_dir(), Some(Path::new("/app")));
    assert_eq!(request.max_stdout_bytes(), 96 * 1024);
}

#[test]
fn invalid_container_exec_shapes_fail_closed() {
    assert!(
        ContainerExecRequest::new(
            OperationId::new(),
            OperationName::new("container.exec").unwrap(),
            "api",
            Vec::new(),
            None,
            None,
            deadline(),
        )
        .is_err()
    );
    assert!(
        ContainerExecRequest::new(
            OperationId::new(),
            OperationName::new("container.exec").unwrap(),
            "api",
            vec![String::from("x"); 257],
            None,
            Some("/app/../root".into()),
            deadline(),
        )
        .is_err()
    );
}
