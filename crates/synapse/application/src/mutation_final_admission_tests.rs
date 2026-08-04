use super::*;

#[test]
fn explicit_deadline_wins_for_final_mutations() {
    let deadline = Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 9_000);
    let context = OperationContext::new().with_deadline(deadline);
    assert_eq!(
        final_execution_deadline(&context, Timestamp::now()),
        deadline
    );
}
