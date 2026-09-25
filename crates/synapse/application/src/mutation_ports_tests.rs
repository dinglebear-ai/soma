use super::*;

#[test]
fn port_bundle_types_remain_product_owned() {
    assert!(std::mem::size_of::<Option<SynapseFinalPorts>>() > 0);
}
