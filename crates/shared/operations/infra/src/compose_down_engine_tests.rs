use super::*;

#[test]
fn teardown_engine_is_zero_sized() {
    assert_eq!(std::mem::size_of::<ComposeDownEngine>(), 0);
}
