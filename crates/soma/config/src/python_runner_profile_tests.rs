use super::{PythonExecutionProfile, PythonRunnerMode};

#[test]
fn defaults_preserve_one_shot_trusted_execution() {
    assert_eq!(PythonRunnerMode::default(), PythonRunnerMode::OneShot);
    assert_eq!(
        PythonExecutionProfile::default(),
        PythonExecutionProfile::Trusted
    );
}
