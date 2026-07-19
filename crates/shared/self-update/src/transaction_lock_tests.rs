use tempfile::tempdir;

use super::*;

#[test]
fn same_executable_different_states_share_lock_and_persistent_authority() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let first_state = temp.path().join("first.json");
    let second_state = temp.path().join("second.json");
    std::fs::write(&executable, b"old").unwrap();
    let first = Updater::new(
        UpdateLayout::new(&executable, &first_state),
        UpdatePolicy::default(),
    );
    let second = Updater::new(
        UpdateLayout::new(&executable, &second_state),
        UpdatePolicy::default(),
    );
    let first_paths = first.validated_layout().unwrap();
    let second_paths = second.validated_layout().unwrap();

    let first_locks = first.transaction_locks(&first_paths).unwrap();
    assert!(matches!(
        second.transaction_locks(&second_paths),
        Err(UpdateError::UpdateInProgress { .. })
    ));
    drop(first_locks);
    let reconstructed = Updater::new(
        UpdateLayout::new(&executable, &second_state),
        UpdatePolicy::default(),
    );
    let reconstructed_paths = reconstructed.validated_layout().unwrap();
    assert!(matches!(
        reconstructed.transaction_locks(&reconstructed_paths),
        Err(UpdateError::InvalidLayout { .. })
    ));
}

#[test]
fn same_state_different_executables_remain_serialized() {
    let temp = tempdir().unwrap();
    let first_executable = temp.path().join("first-agent");
    let second_executable = temp.path().join("second-agent");
    let state = temp.path().join("update.json");
    std::fs::write(&first_executable, b"old").unwrap();
    std::fs::write(&second_executable, b"old").unwrap();
    let first = Updater::new(
        UpdateLayout::new(&first_executable, &state),
        UpdatePolicy::default(),
    );
    let second = Updater::new(
        UpdateLayout::new(&second_executable, &state),
        UpdatePolicy::default(),
    );
    let first_paths = first.validated_layout().unwrap();
    let second_paths = second.validated_layout().unwrap();

    let _first_locks = first.transaction_locks(&first_paths).unwrap();
    assert!(matches!(
        second.transaction_locks(&second_paths),
        Err(UpdateError::UpdateInProgress { .. })
    ));
}
