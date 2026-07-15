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
    let spec = StdioProcessSpec {
        command: "node".to_owned(),
        args: Vec::new(),
        env: [("LD_PRELOAD".to_owned(), "x.so".to_owned())].into(),
    };
    assert!(matches!(
        spec.validate(&SpawnGuard::default()).unwrap_err(),
        StdioSpecError::Env(_)
    ));
}
