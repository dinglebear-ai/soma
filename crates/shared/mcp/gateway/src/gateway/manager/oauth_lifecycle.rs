use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use soma_mcp_client::oauth::{BeginAuthorization, UpstreamOAuthManager};

use super::{GatewayManager, GatewayManagerError};

const TOKEN_EXPIRY_WARNING_SECS: i64 = 300;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpstreamOauthStatusView {
    pub authenticated: bool,
    pub upstream: String,
    pub state: UpstreamOauthConnectionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token_expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seconds_until_expiry: Option<i64>,
    #[serde(default)]
    pub refresh_token_present: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamOauthConnectionState {
    Connected,
    Expiring,
    Expired,
    Disconnected,
}

impl GatewayManager {
    pub async fn begin_upstream_authorization(
        &self,
        upstream: &str,
        subject: &str,
    ) -> Result<BeginAuthorization, GatewayManagerError> {
        self.require_oauth_manager(upstream)?
            .begin_authorization(subject)
            .await
            .map_err(|error| GatewayManagerError::OAuth(error.to_string()))
    }

    pub async fn upstream_oauth_status(
        &self,
        upstream: &str,
        subject: &str,
    ) -> Result<UpstreamOauthStatusView, GatewayManagerError> {
        let manager = self.require_oauth_manager(upstream)?;
        let status = manager
            .credential_status(subject)
            .await
            .map_err(|error| GatewayManagerError::OAuth(error.to_string()))?;
        let Some(status) = status else {
            return Ok(UpstreamOauthStatusView {
                authenticated: false,
                upstream: upstream.to_owned(),
                state: UpstreamOauthConnectionState::Disconnected,
                access_token_expires_at: None,
                seconds_until_expiry: None,
                refresh_token_present: false,
            });
        };
        let now = now_unix()?;
        let seconds = status.access_token_expires_at.saturating_sub(now);
        let state = if status.access_token_expires_at <= now {
            UpstreamOauthConnectionState::Expired
        } else if seconds <= TOKEN_EXPIRY_WARNING_SECS {
            UpstreamOauthConnectionState::Expiring
        } else {
            UpstreamOauthConnectionState::Connected
        };
        Ok(UpstreamOauthStatusView {
            authenticated: matches!(
                state,
                UpstreamOauthConnectionState::Connected | UpstreamOauthConnectionState::Expiring
            ),
            upstream: upstream.to_owned(),
            state,
            access_token_expires_at: Some(status.access_token_expires_at),
            seconds_until_expiry: Some(seconds),
            refresh_token_present: status.refresh_token_present,
        })
    }

    pub async fn clear_upstream_credentials(
        &self,
        upstream: &str,
        subject: &str,
    ) -> Result<(), GatewayManagerError> {
        let manager = self.require_oauth_manager(upstream)?;
        manager
            .clear_credentials(subject)
            .await
            .map_err(|error| GatewayManagerError::OAuth(error.to_string()))?;
        if let Some(runtime) = self
            .oauth_runtime
            .read()
            .expect("gateway oauth runtime poisoned")
            .as_ref()
        {
            runtime.evict_subject(upstream, subject);
            self.pool
                .read()
                .expect("gateway pool poisoned")
                .evict_oauth_subject(upstream, subject);
        }
        Ok(())
    }

    fn require_oauth_manager(
        &self,
        upstream: &str,
    ) -> Result<std::sync::Arc<dyn UpstreamOAuthManager>, GatewayManagerError> {
        let runtime = self
            .oauth_runtime
            .read()
            .expect("gateway oauth runtime poisoned")
            .clone()
            .ok_or_else(|| {
                GatewayManagerError::OAuth("upstream OAuth runtime is not configured".to_owned())
            })?;
        runtime.manager(upstream).ok_or_else(|| {
            GatewayManagerError::OAuth(format!("upstream `{upstream}` has no OAuth manager"))
        })
    }
}

fn now_unix() -> Result<i64, GatewayManagerError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .map_err(|error| GatewayManagerError::OAuth(format!("system clock error: {error}")))
}

#[cfg(test)]
#[path = "oauth_lifecycle_tests.rs"]
mod tests;
