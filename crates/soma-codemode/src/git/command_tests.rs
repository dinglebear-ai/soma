use super::command::GitCommand;

#[test]
fn git_command_uses_fixed_argv() {
    let command = GitCommand::new(".", ["status", "--short"]);
    assert_eq!(
        command.args(),
        &["status".to_string(), "--short".to_string()]
    );
}
