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

#[test]
fn allows_bun_as_a_guarded_runtime() {
    SpawnGuard::default().validate_command("bun").unwrap();
    SpawnGuard::default()
        .validate_args("bun", &["run".to_owned(), "server.ts".to_owned()])
        .unwrap();
}

#[test]
fn rejects_control_characters_in_any_runtime_argument() {
    assert_eq!(
        SpawnGuard::default()
            .validate_args("node", &["server.js\n--inspect".to_owned()])
            .unwrap_err(),
        SpawnGuardError::InvalidArgument
    );
}

#[test]
fn rejects_runtime_specific_code_execution_and_host_access_flags() {
    let cases = [
        ("docker", "--privileged"),
        ("docker", "--network=host"),
        ("docker", "-v/tmp:/host"),
        ("node", "--require=evil.js"),
        ("node", "--inspect-brk"),
        ("node", "-eprocess.exit()"),
        ("npx", "--allow-fs-read"),
        ("bun", "--import=evil.js"),
        ("bun", "-rpreload.js"),
        ("python", "-cprint(1)"),
        ("python3", "-"),
        ("deno", "--allow-all"),
        ("deno", "-A"),
    ];

    for (runtime, arg) in cases {
        assert_eq!(
            SpawnGuard::default()
                .validate_args(runtime, &[arg.to_owned()])
                .unwrap_err(),
            SpawnGuardError::DangerousArgument {
                runtime: runtime.to_owned(),
            },
            "expected {runtime} argument {arg:?} to be denied"
        );
    }
}

#[test]
fn allows_normal_mcp_runtime_arguments() {
    let cases = [
        ("docker", vec!["run", "--rm", "mcp/server"]),
        ("node", vec!["server.js", "--port", "3000"]),
        ("npx", vec!["-y", "@modelcontextprotocol/server-filesystem"]),
        ("bun", vec!["run", "server.ts"]),
        ("python", vec!["-m", "mcp_server"]),
        ("deno", vec!["run", "--allow-read", "server.ts"]),
    ];

    for (runtime, args) in cases {
        let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
        SpawnGuard::default().validate_args(runtime, &args).unwrap();
    }
}

#[test]
fn npx_does_not_inherit_node_concatenated_short_flag_matching() {
    SpawnGuard::default()
        .validate_args("npx", &["-p@scope/package".to_owned()])
        .unwrap();
}
