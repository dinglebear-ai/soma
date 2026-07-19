//! Network CRUD.

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::transport::{
    resource_error_or, sync_metadata, Client, Method, RecursionQuery, WithEtag,
};

#[derive(Debug, Clone, Deserialize)]
pub struct Network {
    pub name: String,
    #[serde(rename = "type")]
    pub network_type: String,
    pub managed: bool,
    pub status: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

impl Client {
    pub async fn list_networks(&self, recursion: bool) -> Result<Vec<serde_json::Value>> {
        let recursion_query = RecursionQuery::new(recursion);
        let envelope = self
            .request(
                Method::Get,
                "/1.0/networks",
                &recursion_query.as_query(),
                None,
                None,
            )
            .await?;
        Ok(serde_json::from_value(sync_metadata(envelope, "list")?)?)
    }

    /// Fetches one network by name, along with its ETag for use as a later
    /// `If-Match` precondition.
    pub async fn get_network(&self, name: &str) -> Result<WithEtag<Network>> {
        let path = format!("/1.0/networks/{name}");
        let envelope = self
            .request(Method::Get, &path, &[], None, None)
            .await
            .map_err(|err| resource_error_or(err, name))?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, etag } => Ok(WithEtag {
                value: serde_json::from_value(metadata)?,
                etag,
            }),
            other => Err(Error::InvalidResponse(format!(
                "expected a sync network response, got {other:?}"
            ))),
        }
    }

    /// Creates a network. Synchronous: the network exists by the time this
    /// returns, with no operation to wait on. Verified against
    /// `cmd/incusd/networks.go`'s `networksPost` on the `lxc/incus` `main`
    /// branch, which always returns `response.SyncResponseLocation` -
    /// there is no code path that returns an async operation response, for
    /// any network type/backend.
    pub async fn create_network(&self, params: &serde_json::Value) -> Result<()> {
        self.request(Method::Post, "/1.0/networks", &[], Some(params), None)
            .await?;
        Ok(())
    }

    /// Full replacement update (PUT). `etag`, if provided, is sent as
    /// `If-Match` for optimistic concurrency; a stale ETag surfaces as
    /// `Error::PreconditionFailed`, not the generic `Error::Api`.
    ///
    /// Synchronous, like [`Client::create_network`] - verified against
    /// `cmd/incusd/networks.go`'s `doNetworkUpdate`, which always returns
    /// `response.EmptySyncResponse`.
    pub async fn update_network(
        &self,
        name: &str,
        new_definition: &serde_json::Value,
        etag: Option<&str>,
    ) -> Result<()> {
        let path = format!("/1.0/networks/{name}");
        self.request(Method::Put, &path, &[], Some(new_definition), etag)
            .await
            .map_err(|err| resource_error_or(err, name))?;
        Ok(())
    }

    /// Same as [`Client::update_network`], but takes the `WithEtag` from a
    /// prior [`Client::get_network`] call directly instead of a bare
    /// `etag: Option<&str>` - see `instances::Client::update_instance_guarded`'s
    /// doc comment for why this exists alongside the raw-`etag` version.
    pub async fn update_network_guarded(
        &self,
        fetched: &WithEtag<Network>,
        new_definition: &serde_json::Value,
    ) -> Result<()> {
        self.update_network(&fetched.value().name, new_definition, fetched.etag())
            .await
    }

    /// Synchronous - verified against `cmd/incusd/networks.go`'s
    /// `networkDelete`, which always returns `response.EmptySyncResponse`.
    pub async fn delete_network(&self, name: &str) -> Result<()> {
        let path = format!("/1.0/networks/{name}");
        self.request(Method::Delete, &path, &[], None, None)
            .await
            .map_err(|err| resource_error_or(err, name))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "networks_tests.rs"]
mod tests;
