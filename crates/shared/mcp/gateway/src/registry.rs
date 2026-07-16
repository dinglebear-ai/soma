#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayServiceMeta {
    pub id: String,
    pub display_name: String,
    pub actions: Vec<GatewayServiceAction>,
    pub env: Vec<GatewayEnvVar>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayServiceAction {
    pub name: String,
    pub admin_required: bool,
    pub destructive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayEnvVar {
    pub name: String,
    pub required: bool,
    pub secret: bool,
}

impl GatewayServiceMeta {
    #[must_use]
    pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            actions: Vec::new(),
            env: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_action(mut self, action: GatewayServiceAction) -> Self {
        self.actions.push(action);
        self
    }

    #[must_use]
    pub fn with_env(mut self, env: GatewayEnvVar) -> Self {
        self.env.push(env);
        self
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
