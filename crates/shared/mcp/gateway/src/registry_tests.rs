use super::*;

#[test]
fn gateway_service_meta_is_gateway_local() {
    let meta = GatewayServiceMeta::new("mock", "Mock")
        .with_action(GatewayServiceAction {
            name: "gateway.test".to_owned(),
            admin_required: true,
            destructive: false,
        })
        .with_env(GatewayEnvVar {
            name: "MOCK_TOKEN".to_owned(),
            required: true,
            secret: true,
        });

    assert_eq!(meta.actions[0].name, "gateway.test");
    assert!(meta.env[0].secret);
}
