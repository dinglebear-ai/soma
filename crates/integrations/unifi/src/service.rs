use serde_json::Value;

use crate::error::Result;
use crate::util::truncate_data_array;
use crate::{ActionDispatcher, ActionRequest, UnifiClient};

/// Business-logic facade over [`UnifiClient`]: fixed read endpoints plus
/// dynamic action dispatch via [`ActionDispatcher`].
///
/// Consumers embedding this crate (CLI commands, MCP tools, HTTP handlers)
/// should depend on `UnifiService`, not [`UnifiClient`] directly — it is the
/// stable seam for adding cross-cutting behavior (result shaping, caching,
/// metrics) without touching every call site.
#[derive(Clone)]
pub struct UnifiService {
    client: UnifiClient,
}

impl UnifiService {
    /// Wraps an already-built [`UnifiClient`].
    pub fn new(client: UnifiClient) -> Self {
        Self { client }
    }

    /// Connected clients (wireless and wired).
    ///
    /// # Errors
    /// See [`crate::UnifiError`] for the failure cases this can return.
    pub async fn clients(&self) -> Result<Value> {
        self.client.clients().await
    }

    /// Network devices: APs, switches, gateways.
    ///
    /// # Errors
    /// See [`crate::UnifiError`] for the failure cases this can return.
    pub async fn devices(&self) -> Result<Value> {
        self.client.devices().await
    }

    /// WLAN (WiFi network) configurations.
    ///
    /// # Errors
    /// See [`crate::UnifiError`] for the failure cases this can return.
    pub async fn wlans(&self) -> Result<Value> {
        self.client.wlans().await
    }

    /// Site health summary.
    ///
    /// # Errors
    /// See [`crate::UnifiError`] for the failure cases this can return.
    pub async fn health(&self) -> Result<Value> {
        self.client.health().await
    }

    /// Active alarms / alerts.
    ///
    /// # Errors
    /// See [`crate::UnifiError`] for the failure cases this can return.
    pub async fn alarms(&self) -> Result<Value> {
        self.client.alarms().await
    }

    /// Recent events, truncated to `limit` entries when given.
    ///
    /// # Errors
    /// See [`crate::UnifiError`] for the failure cases this can return.
    pub async fn events(&self, limit: Option<usize>) -> Result<Value> {
        let mut events = self.client.events().await?;
        truncate_data_array(&mut events, limit);
        Ok(events)
    }

    /// Controller system info.
    ///
    /// # Errors
    /// See [`crate::UnifiError`] for the failure cases this can return.
    pub async fn sysinfo(&self) -> Result<Value> {
        self.client.sysinfo().await
    }

    /// Authenticated user info.
    ///
    /// # Errors
    /// See [`crate::UnifiError`] for the failure cases this can return.
    pub async fn me(&self) -> Result<Value> {
        self.client.me().await
    }

    /// Runs a named, dynamically-dispatched action (see [`ActionDispatcher`]).
    ///
    /// # Errors
    /// See [`crate::UnifiError`] for the failure cases this can return.
    pub async fn execute(&self, request: ActionRequest) -> Result<Value> {
        ActionDispatcher::new(self.client.clone())
            .execute(request)
            .await
    }
}
