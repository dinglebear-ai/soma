use super::*;

#[test]
fn filesystem_operands_follow_typed_command_grammars() {
    assert_eq!(
        filesystem_operand_indices(
            HostExecCommand::Head,
            &["-n".into(), "10".into(), "/srv/app.log".into()],
        )
        .unwrap(),
        vec![2]
    );
    assert_eq!(
        filesystem_operand_indices(HostExecCommand::Rg, &["needle".into(), "/srv".into()],)
            .unwrap(),
        vec![1]
    );
}

#[test]
fn helper_executing_and_unknown_options_fail_closed() {
    assert!(
        filesystem_operand_indices(
            HostExecCommand::Grep,
            &["--include-from=/tmp/options".into(), "x".into()],
        )
        .is_err()
    );
    assert!(filesystem_operand_indices(HostExecCommand::Hostname, &["--help".into()]).is_err());
    assert!(filesystem_operand_indices(HostExecCommand::Rg, &[]).is_err());
}
