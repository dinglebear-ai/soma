use std::path::PathBuf;

use soma_provider_core::{ProviderCatalog, ProviderError};

use super::{
    PythonInterpreter, PythonSupervisorConfig, PythonWorkerIdentity, PythonWorkerSupervisor,
    sha256_hex,
};

/// Discover a Python catalog inside the same supervised containment boundary
/// used for activation. This prevents brokered mode from importing provider
/// code in the host process or an ambient one-shot subprocess.
pub async fn describe_persistent_catalog(
    path: PathBuf,
    interpreter: PythonInterpreter,
    config: PythonSupervisorConfig,
) -> Result<ProviderCatalog, ProviderError> {
    let source = std::fs::read(&path)
        .map_err(|error| ProviderError::execution("", "", error).with_phase("catalog-discovery"))?;
    let source_digest = sha256_hex(&source);
    let supervisor = PythonWorkerSupervisor::new_with_capabilities(
        PythonWorkerIdentity {
            path: path.clone(),
            generation_id: format!("discovery-{}", &source_digest[..16]),
            worker_group: source_digest.clone(),
            source_digest,
            catalog_fingerprint: String::new(),
        },
        interpreter,
        config,
        &soma_provider_core::HostCapabilities::default(),
    );
    let described = supervisor.preflight().await.map_err(|error| {
        ProviderError::new(
            error.code(),
            "",
            None,
            error.to_string(),
            "Inspect the contained Python provider discovery worker.",
        )
        .with_provider_kind("python")
        .with_source(path.display().to_string())
        .with_phase("catalog-discovery")
    });
    supervisor.shutdown().await;
    soma_provider_core::validate_provider_manifest_value(&described?).map_err(|error| {
        ProviderError::validation("", "", "python_catalog_invalid", error.to_string())
            .with_provider_kind("python")
            .with_source(path.display().to_string())
            .with_phase("catalog-discovery")
    })
}
