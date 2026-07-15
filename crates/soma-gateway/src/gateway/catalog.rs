use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayAction {
    pub name: &'static str,
    pub admin_required: bool,
    pub destructive: bool,
    pub discovery: bool,
    pub spawn_validation_required: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GatewayActionCatalog {
    actions: BTreeMap<&'static str, GatewayAction>,
}

impl GatewayActionCatalog {
    #[must_use]
    pub fn standard() -> Self {
        let mut catalog = Self::default();
        for action in standard_actions() {
            catalog.actions.insert(action.name, action);
        }
        catalog
    }

    #[must_use]
    pub fn get(&self, name: &str) -> GatewayAction {
        self.actions.get(name).copied().unwrap_or(GatewayAction {
            name: "unknown",
            admin_required: true,
            destructive: true,
            discovery: false,
            spawn_validation_required: false,
        })
    }

    #[must_use]
    pub fn list(&self) -> Vec<GatewayAction> {
        self.actions.values().copied().collect()
    }
}

fn standard_actions() -> [GatewayAction; 7] {
    [
        GatewayAction {
            name: "gateway.list",
            admin_required: false,
            destructive: false,
            discovery: true,
            spawn_validation_required: false,
        },
        GatewayAction {
            name: "gateway.config.view",
            admin_required: false,
            destructive: false,
            discovery: true,
            spawn_validation_required: false,
        },
        GatewayAction {
            name: "gateway.test",
            admin_required: true,
            destructive: false,
            discovery: false,
            spawn_validation_required: true,
        },
        GatewayAction {
            name: "gateway.add",
            admin_required: true,
            destructive: false,
            discovery: false,
            spawn_validation_required: true,
        },
        GatewayAction {
            name: "gateway.update",
            admin_required: true,
            destructive: false,
            discovery: false,
            spawn_validation_required: true,
        },
        GatewayAction {
            name: "gateway.import.approve",
            admin_required: true,
            destructive: false,
            discovery: false,
            spawn_validation_required: true,
        },
        GatewayAction {
            name: "gateway.remove",
            admin_required: true,
            destructive: true,
            discovery: false,
            spawn_validation_required: false,
        },
    ]
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
