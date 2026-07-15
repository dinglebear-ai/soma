use super::state::DriveState;

#[test]
fn drive_state_allocates_monotonic_seq() {
    let mut state = DriveState::default();
    assert_eq!(state.next_sequence(), 0);
    assert_eq!(state.next_sequence(), 1);
}
