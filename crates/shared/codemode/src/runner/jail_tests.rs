use super::jail::{cleanup_execution_jail, reset_execution_jail};

#[test]
fn jail_reset_and_cleanup_are_idempotent() {
    reset_execution_jail();
    cleanup_execution_jail(false);
    cleanup_execution_jail(true);
}
