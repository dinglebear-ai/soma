use std::{collections::BTreeMap, sync::Arc};

use jsonschema::Validator;

use crate::{
    ProviderCatalog, ProviderId, ProviderValidationError, ToolSpec, validate_provider_manifest,
};

#[derive(Clone)]
pub struct RegisteredTool {
    provider_id: ProviderId,
    tool: ToolSpec,
    pub(super) input_validator: Arc<Validator>,
    pub(super) output_validator: Option<Arc<Validator>>,
}

impl RegisteredTool {
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    pub fn spec(&self) -> &ToolSpec {
        &self.tool
    }
}

#[derive(Clone, Default)]
pub struct ProviderIndexes {
    tools: BTreeMap<String, RegisteredTool>,
    rest: BTreeMap<(String, String), String>,
    cli: BTreeMap<String, String>,
    primitives: BTreeMap<String, &'static str>,
}

impl ProviderIndexes {
    pub(super) fn build(catalogs: &[ProviderCatalog]) -> Result<Self, ProviderValidationError> {
        let mut indexes = Self::default();
        for catalog in catalogs {
            validate_provider_manifest(catalog)?;
            let provider_id = ProviderId::new(&catalog.provider.name).map_err(|error| {
                ProviderValidationError::new("invalid_provider_name", error.to_string())
            })?;
            for tool in &catalog.tools {
                indexes.insert_tool(provider_id.clone(), tool.clone())?;
            }
            for prompt in &catalog.prompts {
                indexes.insert_primitive(&prompt.name, "prompt")?;
            }
            for resource in &catalog.resources {
                indexes.insert_primitive(&resource.name, "resource")?;
            }
            for task in &catalog.tasks {
                indexes.insert_primitive(&task.name, "task")?;
            }
            for elicitation in &catalog.elicitation {
                indexes.insert_primitive(&elicitation.name, "elicitation")?;
            }
        }
        Ok(indexes)
    }

    fn insert_tool(
        &mut self,
        provider_id: ProviderId,
        tool: ToolSpec,
    ) -> Result<(), ProviderValidationError> {
        let input_validator = Arc::new(jsonschema::validator_for(&tool.input_schema).map_err(
            |error| {
                ProviderValidationError::new(
                    "input_schema_invalid",
                    format!("tool `{}` has invalid input_schema: {error}", tool.name),
                )
            },
        )?);
        let output_validator = tool
            .output_schema
            .as_ref()
            .map(jsonschema::validator_for)
            .transpose()
            .map_err(|error| {
                ProviderValidationError::new(
                    "output_schema_invalid",
                    format!("tool `{}` has invalid output_schema: {error}", tool.name),
                )
            })?
            .map(Arc::new);
        let name = tool.name.clone();
        let entry = RegisteredTool {
            provider_id,
            tool: tool.clone(),
            input_validator,
            output_validator,
        };
        if self.tools.insert(name.clone(), entry).is_some() {
            return Err(ProviderValidationError::new(
                "duplicate_tool_name",
                format!("duplicate action `{name}`"),
            ));
        }

        if let Some(rest) = &tool.rest
            && rest.enabled
        {
            let method = rest.method.clone().unwrap_or_else(|| "POST".to_owned());
            let path = rest.path.clone().unwrap_or_else(|| format!("/v1/{name}"));
            if self
                .rest
                .insert((method.clone(), path.clone()), name.clone())
                .is_some()
            {
                return Err(ProviderValidationError::new(
                    "duplicate_rest_route",
                    format!("duplicate REST route {method} {path}"),
                ));
            }
        }
        if let Some(cli) = &tool.cli
            && cli.enabled
        {
            let command = cli.command.clone().unwrap_or_else(|| name.clone());
            self.insert_cli(command, &name)?;
            for alias in &cli.aliases {
                self.insert_cli(alias.clone(), &name)?;
            }
        }
        Ok(())
    }

    fn insert_cli(&mut self, command: String, action: &str) -> Result<(), ProviderValidationError> {
        if self
            .cli
            .insert(command.clone(), action.to_owned())
            .is_some()
        {
            return Err(ProviderValidationError::new(
                "duplicate_cli_command",
                format!("duplicate CLI command `{command}`"),
            ));
        }
        Ok(())
    }

    fn insert_primitive(
        &mut self,
        name: &str,
        kind: &'static str,
    ) -> Result<(), ProviderValidationError> {
        if self.primitives.insert(name.to_owned(), kind).is_some() {
            return Err(ProviderValidationError::new(
                "duplicate_mcp_primitive",
                format!("duplicate MCP primitive `{name}`"),
            ));
        }
        Ok(())
    }

    pub fn tool(&self, action: &str) -> Option<&RegisteredTool> {
        self.tools.get(action)
    }

    pub fn action_names(&self) -> impl Iterator<Item = &str> {
        self.tools.keys().map(String::as_str)
    }

    pub fn route_action(&self, method: &str, path: &str) -> Option<&str> {
        self.rest
            .get(&(method.to_owned(), path.to_owned()))
            .map(String::as_str)
    }

    pub fn cli_action(&self, command: &str) -> Option<&str> {
        self.cli.get(command).map(String::as_str)
    }

    pub fn primitive_kind(&self, name: &str) -> Option<&str> {
        self.primitives.get(name).copied()
    }

    pub fn rest_routes(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.rest
            .iter()
            .map(|((method, path), action)| (method.as_str(), path.as_str(), action.as_str()))
    }

    pub fn compiled_validator_count(&self) -> usize {
        self.tools
            .values()
            .map(|tool| 1 + usize::from(tool.output_validator.is_some()))
            .sum()
    }
}
