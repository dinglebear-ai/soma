use soma_ops::{OperationContext, Timestamp};

use super::*;

#[test]
fn explicit_replacement_deadline_wins_over_default_budget() {
    let started = Timestamp::from_unix_millis(1_000);
    let explicit = Timestamp::from_unix_millis(9_000);
    let context = OperationContext::new().with_deadline(explicit);
    assert_eq!(deadline(&context, started), explicit);
}
