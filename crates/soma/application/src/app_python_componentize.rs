use std::path::{Path, PathBuf};

use serde_json::Value;
use soma_provider_core::ProviderCatalog;

pub(super) fn graduation_status(workspace: &Path, provider_root: &Path) -> anyhow::Result<Value> {
    let mut graduation = crate::graduation::status(workspace, provider_root)?;
    let componentize = crate::componentize::status(workspace, provider_root)?;
    graduation
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("graduation status must be an object"))?
        .insert("componentize".to_owned(), componentize);
    Ok(graduation)
}

pub(super) struct GraduationOperation {
    pub(super) operation: String,
    pub(super) workspace: PathBuf,
    pub(super) source: Option<PathBuf>,
    pub(super) component: Option<PathBuf>,
    pub(super) fixtures: Option<PathBuf>,
    pub(super) wheelhouse: Option<PathBuf>,
    pub(super) catalog: Option<ProviderCatalog>,
    pub(super) provider_root: PathBuf,
}

impl GraduationOperation {
    pub(super) fn run(self) -> anyhow::Result<Value> {
        let Self {
            operation,
            workspace,
            source,
            component,
            fixtures,
            wheelhouse,
            catalog,
            provider_root,
        } = self;
        match operation.as_str() {
            "graduate" => crate::graduation::graduate(
                source
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("graduate requires source"))?,
                &workspace,
                fixtures.as_deref(),
                catalog.expect("graduate catalog prepared"),
                &provider_root,
            ),
            "build-component" => {
                crate::graduation::build_component(&workspace, component.as_deref(), &provider_root)
            }
            "verify-component" => crate::graduation::verify_component(
                component
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("verify-component requires component"))?,
            ),
            "componentize-scan" => {
                crate::componentize::scan(&workspace, wheelhouse.as_deref(), &provider_root)
            }
            "componentize-bindings" => crate::componentize::bindings(&workspace, &provider_root),
            "componentize-build" => crate::componentize::build(&workspace, &provider_root),
            "componentize-validate" => crate::componentize::validate(&workspace, &provider_root),
            "activate" => crate::graduation::activate(&workspace, &provider_root),
            "rollback" => crate::graduation::rollback(&workspace, &provider_root),
            _ => anyhow::bail!("unknown graduation operation"),
        }
    }
}
