use super::*;
#[test]
fn explicit_deadline_wins_for_builds() {
    let value = Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 1234);
    let context = OperationContext::new().with_deadline(value);
    assert_eq!(deadline(&context, Timestamp::now()), value);
}
