use soma_ops::{OperationContext, Timestamp};

use super::*;

#[test]
fn execution_deadline_uses_the_earliest_bound() {
    let started = Timestamp::from_unix_millis(1_000);
    let context = OperationContext::new().with_deadline(Timestamp::from_unix_millis(3_000));
    assert_eq!(
        bounded_deadline(&context, started, 10_000),
        Timestamp::from_unix_millis(3_000)
    );
    assert_eq!(
        bounded_deadline(&OperationContext::new(), started, 500),
        Timestamp::from_unix_millis(1_500)
    );
}
