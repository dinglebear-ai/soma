use super::outcome::DriveOutcome;

#[test]
fn outcome_is_copyable() {
    let outcome = DriveOutcome::Done;
    assert_eq!(outcome, DriveOutcome::Done);
}
