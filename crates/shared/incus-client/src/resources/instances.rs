//! Instance (container/VM) CRUD, lifecycle, and snapshots.
//!
//! Exec, console attach, and file push/pull are deliberately **not**
//! implemented here: each uses `POST .../exec`-style operations whose
//! `metadata` carries secrets for separate control/stdin/stdout WebSocket
//! connections - a materially different protocol from the generic
//! operations/events model the rest of this crate is built on. That's
//! follow-up work for whenever a real consumer needs it, not a gap in this
//! epic.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::operations::{operation_from_envelope, Operation};
use crate::transport::{
    resource_error_or, sync_metadata, Client, Method, RecursionQuery, WithEtag,
};

/// A container or virtual machine. `config`/`devices` stay untyped
/// (`serde_json::Value`) - Incus's instance config schema is large and
/// mostly free-form key-value pairs, so fully typing it is out of scope for
/// this crate.
#[derive(Debug, Clone, Deserialize)]
pub struct Instance {
    pub name: String,
    pub status: String,
    pub status_code: u16,
    #[serde(rename = "type")]
    pub instance_type: String,
    pub architecture: String,
    pub created_at: String,
    pub last_used_at: String,
    pub location: String,
    pub project: String,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub devices: serde_json::Value,
    #[serde(default)]
    pub profiles: Vec<String>,
}

/// Parameters for [`Client::create_instance`]. `source` is the raw Incus
/// source-object JSON (e.g. `{"type": "image", "fingerprint": "..."}`) -
/// kept untyped since Incus supports several distinct source shapes
/// (image, copy, migration, none) that aren't worth fully typing for v1.
#[derive(Debug, Clone, Serialize)]
pub struct CreateInstanceParams {
    pub name: String,
    #[serde(rename = "type")]
    pub instance_type: String,
    pub source: serde_json::Value,
}

impl Client {
    /// Lists instances. `recursion = true` fetches every instance's full
    /// object (config/devices/state) in one call and can be expensive on
    /// hosts with many instances; `recursion = false` returns lightweight
    /// name/URL references only.
    pub async fn list_instances(&self, recursion: bool) -> Result<Vec<serde_json::Value>> {
        let recursion_query = RecursionQuery::new(recursion);
        let envelope = self
            .request(
                Method::Get,
                "/1.0/instances",
                &recursion_query.as_query(),
                None,
                None,
            )
            .await?;
        Ok(serde_json::from_value(sync_metadata(envelope, "list")?)?)
    }

    /// Fetches one instance by name, along with its ETag for use as a later
    /// `If-Match` precondition.
    pub async fn get_instance(&self, name: &str) -> Result<WithEtag<Instance>> {
        let path = format!("/1.0/instances/{name}");
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
                "expected a sync instance response, got {other:?}"
            ))),
        }
    }

    /// Creates an instance. Always async, per Incus's documented behavior
    /// for instance creation.
    pub async fn create_instance(&self, params: &CreateInstanceParams) -> Result<Operation> {
        let body = serde_json::to_value(params)?;
        let envelope = self
            .request(Method::Post, "/1.0/instances", &[], Some(&body), None)
            .await?;
        operation_from_envelope(envelope)
    }

    /// Full replacement update (PUT). `etag`, if provided, is sent as
    /// `If-Match` for optimistic concurrency; a stale ETag surfaces as
    /// `Error::PreconditionFailed`, not the generic `Error::Api`.
    pub async fn update_instance(
        &self,
        name: &str,
        new_definition: &serde_json::Value,
        etag: Option<&str>,
    ) -> Result<Operation> {
        let path = format!("/1.0/instances/{name}");
        let envelope = self
            .request(Method::Put, &path, &[], Some(new_definition), etag)
            .await
            .map_err(|err| resource_error_or(err, name))?;
        operation_from_envelope(envelope)
    }

    /// Same as [`Client::update_instance`], but takes the `WithEtag` from a
    /// prior [`Client::get_instance`] call directly instead of a bare
    /// `etag: Option<&str>` - the "pit of success" version of the
    /// fetch-then-guarded-update workflow, since it makes threading a
    /// genuinely-fetched ETag the natural path rather than something a
    /// caller has to remember to do by hand. `update_instance` remains
    /// available directly for callers with a legitimate reason to supply
    /// their own ETag (e.g. one persisted from a previous process).
    pub async fn update_instance_guarded(
        &self,
        fetched: &WithEtag<Instance>,
        new_definition: &serde_json::Value,
    ) -> Result<Operation> {
        self.update_instance(&fetched.value().name, new_definition, fetched.etag())
            .await
    }

    /// Partial update (PATCH) - use this instead of `update_instance` for
    /// small config changes, to avoid a GET-then-PUT round trip.
    pub async fn patch_instance(
        &self,
        name: &str,
        patch: &serde_json::Value,
        etag: Option<&str>,
    ) -> Result<Operation> {
        let path = format!("/1.0/instances/{name}");
        let envelope = self
            .request(Method::Patch, &path, &[], Some(patch), etag)
            .await
            .map_err(|err| resource_error_or(err, name))?;
        operation_from_envelope(envelope)
    }

    /// Same as [`Client::patch_instance`], but takes the `WithEtag` from a
    /// prior [`Client::get_instance`] call directly - see
    /// [`Client::update_instance_guarded`]'s doc comment for why this
    /// exists alongside the raw-`etag` version.
    pub async fn patch_instance_guarded(
        &self,
        fetched: &WithEtag<Instance>,
        patch: &serde_json::Value,
    ) -> Result<Operation> {
        self.patch_instance(&fetched.value().name, patch, fetched.etag())
            .await
    }

    pub async fn delete_instance(&self, name: &str) -> Result<Operation> {
        let path = format!("/1.0/instances/{name}");
        let envelope = self
            .request(Method::Delete, &path, &[], None, None)
            .await
            .map_err(|err| resource_error_or(err, name))?;
        operation_from_envelope(envelope)
    }

    async fn set_state(&self, name: &str, action: &str) -> Result<Operation> {
        let path = format!("/1.0/instances/{name}/state");
        let body = serde_json::json!({ "action": action });
        let envelope = self
            .request(Method::Put, &path, &[], Some(&body), None)
            .await?;
        operation_from_envelope(envelope)
    }

    pub async fn start_instance(&self, name: &str) -> Result<Operation> {
        self.set_state(name, "start").await
    }

    pub async fn stop_instance(&self, name: &str) -> Result<Operation> {
        self.set_state(name, "stop").await
    }

    pub async fn restart_instance(&self, name: &str) -> Result<Operation> {
        self.set_state(name, "restart").await
    }

    pub async fn pause_instance(&self, name: &str) -> Result<Operation> {
        self.set_state(name, "freeze").await
    }

    pub async fn list_snapshots(
        &self,
        instance_name: &str,
        recursion: bool,
    ) -> Result<Vec<serde_json::Value>> {
        let recursion_query = RecursionQuery::new(recursion);
        let path = format!("/1.0/instances/{instance_name}/snapshots");
        let envelope = self
            .request(Method::Get, &path, &recursion_query.as_query(), None, None)
            .await?;
        Ok(serde_json::from_value(sync_metadata(envelope, "list")?)?)
    }

    pub async fn create_snapshot(
        &self,
        instance_name: &str,
        snapshot_name: &str,
    ) -> Result<Operation> {
        let path = format!("/1.0/instances/{instance_name}/snapshots");
        let body = serde_json::json!({ "name": snapshot_name });
        let envelope = self
            .request(Method::Post, &path, &[], Some(&body), None)
            .await?;
        operation_from_envelope(envelope)
    }

    pub async fn delete_snapshot(
        &self,
        instance_name: &str,
        snapshot_name: &str,
    ) -> Result<Operation> {
        let path = format!("/1.0/instances/{instance_name}/snapshots/{snapshot_name}");
        let envelope = self
            .request(Method::Delete, &path, &[], None, None)
            .await
            .map_err(|err| resource_error_or(err, snapshot_name))?;
        operation_from_envelope(envelope)
    }
}

#[cfg(test)]
#[path = "instances_tests.rs"]
mod tests;
