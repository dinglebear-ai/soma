use std::collections::HashMap;
use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncReadExt;

use crate::config::{OpenApiConfig, OpenApiCredential, OpenApiSpecConfig, SpecSource};
use crate::error::OpenApiError;

pub const MAX_SPECS: usize = 10;
pub const MAX_OPERATIONS_PER_SPEC: usize = 200;
pub const MAX_SPEC_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct OperationHandle {
    pub operation_id: String,
    pub method: reqwest::Method,
    pub path_template: String,
    pub base_url: url::Url,
    pub credential: Option<OpenApiCredential>,
}

#[derive(Debug, Clone)]
pub struct SpecEntry {
    pub operations: HashMap<String, OperationHandle>,
}

#[derive(Clone, Default)]
pub struct OpenApiRegistry {
    inner: Arc<HashMap<String, SpecEntry>>,
}

impl OpenApiRegistry {
    pub async fn from_config(cfg: OpenApiConfig, per_spec_timeout: Duration) -> Self {
        Self::load(cfg, per_spec_timeout).await
    }

    pub async fn load(cfg: OpenApiConfig, per_spec_timeout: Duration) -> Self {
        let total = cfg.specs.len();
        let specs: Vec<_> = cfg.specs.into_iter().take(MAX_SPECS).collect();
        if total > MAX_SPECS {
            tracing::warn!(
                service = "openapi",
                kept = MAX_SPECS,
                configured = total,
                "openapi: MAX_SPECS exceeded, extra specs dropped"
            );
        }

        let loads = specs.into_iter().map(|spec| async move {
            let label = spec.label.clone();
            match tokio::time::timeout(per_spec_timeout, load_one_spec(spec)).await {
                Ok(Ok(entry)) => Some((label, entry)),
                Ok(Err(error)) => {
                    tracing::warn!(
                        service = "openapi",
                        label = %label,
                        kind = error.kind(),
                        "openapi spec omitted: load failed"
                    );
                    None
                }
                Err(_) => {
                    tracing::warn!(
                        service = "openapi",
                        label = %label,
                        kind = "timeout",
                        "openapi spec omitted: load timed out"
                    );
                    None
                }
            }
        });

        let map = futures::future::join_all(loads)
            .await
            .into_iter()
            .flatten()
            .collect();
        Self {
            inner: Arc::new(map),
        }
    }

    #[cfg(test)]
    #[must_use]
    pub fn from_map_for_test(map: HashMap<String, SpecEntry>) -> Self {
        Self {
            inner: Arc::new(map),
        }
    }

    #[must_use]
    pub fn labels(&self) -> Vec<String> {
        let mut labels: Vec<_> = self.inner.keys().cloned().collect();
        labels.sort();
        labels
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn operation(&self, label: &str, op: &str) -> Result<&OperationHandle, OpenApiError> {
        let entry = self
            .inner
            .get(label)
            .ok_or_else(|| OpenApiError::UnknownInstance {
                label: label.to_string(),
                valid: self.labels(),
            })?;
        entry
            .operations
            .get(op)
            .ok_or_else(|| OpenApiError::UnknownOperation {
                label: label.to_string(),
                operation_id: op.to_string(),
            })
    }
}

async fn load_one_spec(spec: OpenApiSpecConfig) -> Result<SpecEntry, OpenApiError> {
    if spec.is_reserved_label() {
        return Err(OpenApiError::SpecParse { label: spec.label });
    }
    let base_url = crate::ssrf::validate_base_url(&spec)?;
    let spec_json = fetch_spec_json(&spec.spec_source, &spec.label).await?;
    let descriptors =
        crate::convert::convert_spec(&spec.label, &spec_json, &spec.allowed_operations)?;
    let converted = descriptors.len();
    let mut operations = HashMap::new();

    for descriptor in descriptors.into_iter().take(MAX_OPERATIONS_PER_SPEC) {
        operations.insert(
            descriptor.operation_id.clone(),
            OperationHandle {
                operation_id: descriptor.operation_id,
                method: descriptor.method,
                path_template: descriptor.path_template,
                base_url: base_url.clone(),
                credential: spec.credential.clone(),
            },
        );
    }

    if converted > MAX_OPERATIONS_PER_SPEC {
        tracing::warn!(
            service = "openapi",
            label = %spec.label,
            kept = MAX_OPERATIONS_PER_SPEC,
            converted,
            "openapi: MAX_OPERATIONS_PER_SPEC exceeded, extra operations dropped"
        );
    }
    if operations.is_empty() {
        tracing::warn!(
            service = "openapi",
            label = %spec.label,
            allowed = spec.allowed_operations.len(),
            kind = "empty_allowlist",
            "openapi spec loaded but no operations matched the allowlist"
        );
    }

    Ok(SpecEntry { operations })
}

async fn fetch_spec_json(source: &SpecSource, label: &str) -> Result<String, OpenApiError> {
    match source {
        SpecSource::Url(url) => {
            crate::ssrf::validate_spec_url(label, url)?;
            crate::http::fetch_url_capped(url, MAX_SPEC_BYTES, label).await
        }
        SpecSource::Path(path) => read_path_capped(path, MAX_SPEC_BYTES, label).await,
    }
}

async fn read_path_capped(
    path: &std::path::Path,
    cap: usize,
    label: &str,
) -> Result<String, OpenApiError> {
    if let Ok(metadata) = tokio::fs::metadata(path).await {
        if metadata.len() > cap as u64 {
            return Err(OpenApiError::SpecTooLarge {
                label: label.to_string(),
            });
        }
    }

    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| OpenApiError::SpecParse {
            label: label.to_string(),
        })?;
    let mut reader = file.take(cap as u64 + 1);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| match error.kind() {
            ErrorKind::InvalidData => OpenApiError::SpecParse {
                label: label.to_string(),
            },
            _ => OpenApiError::SpecParse {
                label: label.to_string(),
            },
        })?;

    if bytes.len() > cap {
        return Err(OpenApiError::SpecTooLarge {
            label: label.to_string(),
        });
    }

    String::from_utf8(bytes).map_err(|_| OpenApiError::SpecParse {
        label: label.to_string(),
    })
}
