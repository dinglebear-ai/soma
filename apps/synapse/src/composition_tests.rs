use super::*;

#[test]
fn default_product_composes_every_runtime_port() {
    let runtime = StandaloneRuntime::from_config(SynapseConfig::default()).unwrap();
    assert_eq!(runtime.catalog().operation_count(), 59);
}
