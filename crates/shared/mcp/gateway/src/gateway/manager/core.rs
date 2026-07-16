use crate::config::{GatewayConfig, UpstreamConfig};

use super::{GatewayManager, GatewayManagerError};

impl GatewayManager {
    pub fn reload(&self, next: GatewayConfig) -> Result<(), GatewayManagerError> {
        self.reload_config(next)
    }

    pub fn upstream_config(&self, name: &str) -> Option<UpstreamConfig> {
        self.config
            .read()
            .expect("gateway config poisoned")
            .upstream
            .iter()
            .find(|upstream| upstream.name == name)
            .cloned()
    }

    #[cfg(feature = "oauth")]
    pub async fn upstream_oauth_access_token(
        &self,
        upstream: &UpstreamConfig,
        subject: &str,
    ) -> Result<Option<String>, GatewayManagerError> {
        if upstream.oauth.is_none() {
            return Ok(None);
        }
        let runtime = self
            .oauth_runtime
            .read()
            .expect("gateway oauth runtime poisoned")
            .clone()
            .ok_or_else(|| {
                GatewayManagerError::OAuth(format!(
                    "upstream `{}` is not connected with OAuth",
                    upstream.name
                ))
            })?;
        runtime
            .manager(&upstream.name)
            .ok_or_else(|| {
                GatewayManagerError::OAuth(format!(
                    "upstream `{}` has no OAuth manager",
                    upstream.name
                ))
            })?
            .access_token(subject)
            .await
            .map(Some)
            .map_err(|error| GatewayManagerError::OAuth(error.to_string()))
    }
}

#[cfg(test)]
#[path = "core_tests.rs"]
mod tests;
