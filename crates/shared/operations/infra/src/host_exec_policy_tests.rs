use soma_ops::{OperationId, OperationName, Timestamp};

use super::*;
use crate::HostExecCommand;

fn request(args: Vec<String>, working_dir: Option<PathBuf>) -> HostExecRequest {
    HostExecRequest::new(
        OperationId::new(),
        OperationName::new("host.exec").unwrap(),
        HostExecCommand::Ls,
        args,
        working_dir,
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap()
}

#[test]
fn launcher_plan_confines_operands_and_working_directory() {
    let policy = HostExecPolicy::new(["/srv", "/tmp"]).unwrap();
    let plan = policy
        .launcher_plan(&request(
            vec!["-l".into(), "/srv/app".into()],
            Some("/srv".into()),
        ))
        .unwrap();
    assert_eq!(plan.path_indices, vec![1]);
    assert_eq!(plan.working_dir.as_deref(), Some("/srv"));
    assert_eq!(plan.roots, vec!["/srv", "/tmp"]);
}

#[test]
fn outside_and_relative_operands_fail_before_execution() {
    let policy = HostExecPolicy::new(["/srv"]).unwrap();
    assert!(
        policy
            .launcher_plan(&request(vec!["/etc/passwd".into()], None))
            .is_err()
    );
    assert!(
        policy
            .launcher_plan(&request(vec!["relative".into()], None))
            .is_err()
    );
}
