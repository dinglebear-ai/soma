use soma_gateway::gateway::catalog::GatewayActionCatalog;

use super::project_gateway_action_catalog;

#[test]
fn projects_every_standard_gateway_action_as_a_tool() {
    let actions = GatewayActionCatalog::standard();
    let catalog = project_gateway_action_catalog("gateway", "Gateway administration", &actions);

    assert_eq!(catalog.tools.len(), actions.list().len());
    let reload = catalog
        .tools
        .iter()
        .find(|tool| tool.name == "gateway.reload")
        .expect("gateway.reload projected");
    assert!(reload.requires_admin);
    assert!(!reload.destructive);

    let remove = catalog
        .tools
        .iter()
        .find(|tool| tool.name == "gateway.remove")
        .expect("gateway.remove projected");
    assert!(remove.destructive);
}
