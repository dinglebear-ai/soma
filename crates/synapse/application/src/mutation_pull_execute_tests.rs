use soma_ops::{OperationContext, Timestamp};

use super::*;

#[test]
fn explicit_context_deadline_wins_over_default_mutation_budget() {
    let deadline = Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 5_000);
    let context = OperationContext::new().with_deadline(deadline);
    assert_eq!(mutation_deadline(&context, Timestamp::now()), deadline);
}
