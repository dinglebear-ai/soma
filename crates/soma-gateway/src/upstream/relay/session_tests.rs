use super::*;

#[test]
fn minted_session_ids_are_unique_and_monotonic() {
    let mint = RelaySessionMint::new();

    let first = mint.mint();
    let second = mint.mint();

    assert_ne!(first, second);
    assert!(second.get() > first.get());
}
