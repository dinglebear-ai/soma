use super::runner_handle::RunnerSpawn;

#[test]
fn runner_spawn_current_exe_has_program() {
    assert!(RunnerSpawn::current_exe().unwrap().program.is_absolute());
}
