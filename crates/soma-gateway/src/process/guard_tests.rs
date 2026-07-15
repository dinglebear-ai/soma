use super::*;

#[test]
fn allows_known_bare_command_names() {
    SpawnGuard::default().validate_command("node").unwrap();
    SpawnGuard::default().validate_command("python3").unwrap();
}

#[test]
fn rejects_path_commands_even_when_basename_is_allowed() {
    assert_eq!(
        SpawnGuard::default()
            .validate_command("/tmp/x/node")
            .unwrap_err(),
        SpawnGuardError::PathCommandDenied
    );
}

#[test]
fn rejects_unknown_commands_unless_extra_allowlisted() {
    assert_eq!(
        SpawnGuard::default()
            .validate_command("custom-mcp")
            .unwrap_err(),
        SpawnGuardError::CommandDenied
    );
    SpawnGuard::default()
        .with_extra(["custom-mcp"])
        .validate_command("custom-mcp")
        .unwrap();
}
