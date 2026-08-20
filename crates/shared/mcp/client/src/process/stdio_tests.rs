use super::*;

#[test]
fn validates_command_env_and_args_together() {
    let spec = StdioProcessSpec {
        command: "node".to_owned(),
        args: vec!["server.js".to_owned()],
        env: [("UPSTREAM_TOKEN".to_owned(), "secret".to_owned())].into(),
    };
    spec.validate(&SpawnGuard::default()).unwrap();
}

#[test]
fn rejects_invalid_env_before_spawn() {
    for (name, value) in [("LD_PRELOAD", "x.so"), ("PATH", "/tmp/evil")] {
        let spec = StdioProcessSpec {
            command: "node".to_owned(),
            args: Vec::new(),
            env: [(name.to_owned(), value.to_owned())].into(),
        };
        assert!(
            matches!(
                spec.validate(&SpawnGuard::default()).unwrap_err(),
                StdioSpecError::Env(_)
            ),
            "expected {name} override to fail before spawn"
        );
    }
}

#[test]
fn rejects_dangerous_runtime_args_before_spawn() {
    let spec = StdioProcessSpec {
        command: "bun".to_owned(),
        args: vec!["-eprocess.exit()".to_owned()],
        env: BTreeMap::new(),
    };

    assert_eq!(
        spec.validate(&SpawnGuard::default()).unwrap_err(),
        StdioSpecError::Guard(SpawnGuardError::DangerousArgument {
            runtime: "bun".to_owned(),
        })
    );
}
