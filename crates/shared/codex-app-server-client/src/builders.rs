use crate::protocol::{
    ClientInfo, ConfigReadParams, InitializeCapabilities, InitializeParams, ThreadStartParams,
    TurnStartParams, UserInput,
};

impl ClientInfo {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: None,
            version: version.into(),
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

impl InitializeParams {
    pub fn for_client(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            capabilities: None,
            client_info: ClientInfo::new(name, version),
        }
    }

    pub fn with_capabilities(mut self, capabilities: InitializeCapabilities) -> Self {
        self.capabilities = Some(capabilities);
        self
    }
}

impl ThreadStartParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn ephemeral(mut self, ephemeral: bool) -> Self {
        self.ephemeral = Some(ephemeral);
        self
    }

    pub fn developer_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.developer_instructions = Some(instructions.into());
        self
    }
}

impl UserInput {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            text_elements: Vec::new(),
        }
    }
}

impl TurnStartParams {
    pub fn text(thread_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            additional_context: None,
            approval_policy: None,
            approvals_reviewer: None,
            client_user_message_id: None,
            collaboration_mode: None,
            cwd: None,
            effort: None,
            environments: None,
            input: vec![UserInput::text(text)],
            model: None,
            multi_agent_mode: None,
            output_schema: None,
            permissions: None,
            personality: None,
            responsesapi_client_metadata: None,
            runtime_workspace_roots: None,
            sandbox_policy: None,
            service_tier: None,
            summary: None,
            thread_id: thread_id.into(),
        }
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }
}

impl ConfigReadParams {
    pub fn for_cwd(cwd: impl Into<String>) -> Self {
        Self {
            cwd: Some(cwd.into()),
            include_layers: None,
        }
    }

    pub fn include_layers(mut self, include_layers: bool) -> Self {
        self.include_layers = Some(include_layers);
        self
    }
}
