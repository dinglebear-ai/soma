#[test]
fn binary_name_is_stable() {
    assert_eq!(env!("CARGO_PKG_NAME"), "synapse");
}
