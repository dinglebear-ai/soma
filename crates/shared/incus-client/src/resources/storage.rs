//! Storage pool and volume CRUD. Volumes are scoped under a pool
//! (`/1.0/storage-pools/{pool}/volumes`) - Incus has no global cross-pool
//! volumes endpoint, so "list all volumes across all pools" is inherently a
//! list-pools-then-list-volumes-per-pool fan-out on the caller's part, not a
//! gap in this crate.

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::operations::{optional_operation_from_envelope, Operation};
use crate::transport::{
    resource_error_or, sync_metadata, Client, Method, RecursionQuery, WithEtag,
};

#[derive(Debug, Clone, Deserialize)]
pub struct StoragePool {
    pub name: String,
    pub driver: String,
    pub status: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageVolume {
    pub name: String,
    #[serde(rename = "type")]
    pub volume_type: String,
    pub content_type: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

impl Client {
    pub async fn list_storage_pools(&self, recursion: bool) -> Result<Vec<serde_json::Value>> {
        let recursion_query = RecursionQuery::new(recursion);
        let envelope = self
            .request(
                Method::Get,
                "/1.0/storage-pools",
                &recursion_query.as_query(),
                None,
                None,
            )
            .await?;
        Ok(serde_json::from_value(sync_metadata(envelope, "list")?)?)
    }

    /// Fetches one storage pool by name, along with its ETag for use as a
    /// later `If-Match` precondition.
    pub async fn get_storage_pool(&self, name: &str) -> Result<WithEtag<StoragePool>> {
        let path = format!("/1.0/storage-pools/{name}");
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
                "expected a sync storage pool response, got {other:?}"
            ))),
        }
    }

    /// Creates a storage pool. Synchronous: the pool exists by the time
    /// this returns, with no operation to wait on. Verified against
    /// `cmd/incusd/storage_pools.go`'s `storagePoolsPost` on the
    /// `lxc/incus` `main` branch, which always returns
    /// `response.SyncResponseLocation`.
    pub async fn create_storage_pool(&self, params: &serde_json::Value) -> Result<()> {
        self.request(Method::Post, "/1.0/storage-pools", &[], Some(params), None)
            .await?;
        Ok(())
    }

    /// Full replacement update (PUT). `etag`, if provided, is sent as
    /// `If-Match` for optimistic concurrency; a stale ETag surfaces as
    /// `Error::PreconditionFailed`, not the generic `Error::Api`.
    ///
    /// Synchronous, like [`Client::create_storage_pool`] - verified against
    /// `cmd/incusd/storage_pools.go`'s `doStoragePoolUpdate`, which always
    /// returns `response.EmptySyncResponse`.
    pub async fn update_storage_pool(
        &self,
        name: &str,
        new_definition: &serde_json::Value,
        etag: Option<&str>,
    ) -> Result<()> {
        let path = format!("/1.0/storage-pools/{name}");
        self.request(Method::Put, &path, &[], Some(new_definition), etag)
            .await
            .map_err(|err| resource_error_or(err, name))?;
        Ok(())
    }

    /// Same as [`Client::update_storage_pool`], but takes the `WithEtag`
    /// from a prior [`Client::get_storage_pool`] call directly instead of a
    /// bare `etag: Option<&str>` - see
    /// `instances::Client::update_instance_guarded`'s doc comment for why
    /// this exists alongside the raw-`etag` version.
    pub async fn update_storage_pool_guarded(
        &self,
        fetched: &WithEtag<StoragePool>,
        new_definition: &serde_json::Value,
    ) -> Result<()> {
        self.update_storage_pool(&fetched.value().name, new_definition, fetched.etag())
            .await
    }

    /// Synchronous - verified against `cmd/incusd/storage_pools.go`'s
    /// `storagePoolDelete`, which always returns `response.EmptySyncResponse`.
    pub async fn delete_storage_pool(&self, name: &str) -> Result<()> {
        let path = format!("/1.0/storage-pools/{name}");
        self.request(Method::Delete, &path, &[], None, None)
            .await
            .map_err(|err| resource_error_or(err, name))?;
        Ok(())
    }

    /// Lists volumes within one pool. See the module doc comment for why
    /// there's no "list all volumes across all pools" convenience method.
    ///
    /// Per the Incus REST API, `recursion = false` returns an array of bare
    /// URL strings (not typed volume objects), so - like every other
    /// `list_*` method in this crate - this returns untyped
    /// `serde_json::Value`s rather than `Vec<StorageVolume>`. Use
    /// `recursion = true` to get full volume objects in one call, or
    /// `get_storage_volume` to fetch one volume's full object by name.
    pub async fn list_storage_volumes(
        &self,
        pool_name: &str,
        recursion: bool,
    ) -> Result<Vec<serde_json::Value>> {
        let recursion_query = RecursionQuery::new(recursion);
        let path = format!("/1.0/storage-pools/{pool_name}/volumes");
        let envelope = self
            .request(Method::Get, &path, &recursion_query.as_query(), None, None)
            .await?;
        Ok(serde_json::from_value(sync_metadata(envelope, "list")?)?)
    }

    /// Creates a volume. Unlike every other create/update/delete method in
    /// this crate, volume creation is genuinely conditional on the request
    /// payload: creating a blank volume (`params` has no `source.name`) is
    /// synchronous, but creating one by copying another volume (`params`
    /// has a `source.name`) is asynchronous. Verified against
    /// `cmd/incusd/storage_volumes.go`'s `doVolumeCreateOrCopy` on the
    /// `lxc/incus` `main` branch: it returns `response.EmptySyncResponse`
    /// when `req.Source.Name == ""`, and `operations.OperationResponse(op)`
    /// otherwise. Returns `None` for the synchronous case (nothing to wait
    /// for) and `Some(operation)` for the asynchronous one.
    pub async fn create_storage_volume(
        &self,
        pool_name: &str,
        params: &serde_json::Value,
    ) -> Result<Option<Operation>> {
        let path = format!("/1.0/storage-pools/{pool_name}/volumes");
        let envelope = self
            .request(Method::Post, &path, &[], Some(params), None)
            .await?;
        optional_operation_from_envelope(envelope)
    }

    /// Fetches one volume's full object by pool, type, and name, along with
    /// its ETag for use as a later `If-Match` precondition.
    pub async fn get_storage_volume(
        &self,
        pool_name: &str,
        volume_type: &str,
        volume_name: &str,
    ) -> Result<WithEtag<StorageVolume>> {
        let path = format!("/1.0/storage-pools/{pool_name}/volumes/{volume_type}/{volume_name}");
        let envelope = self
            .request(Method::Get, &path, &[], None, None)
            .await
            .map_err(|err| resource_error_or(err, volume_name))?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, etag } => Ok(WithEtag {
                value: serde_json::from_value(metadata)?,
                etag,
            }),
            other => Err(Error::InvalidResponse(format!(
                "expected a sync storage volume response, got {other:?}"
            ))),
        }
    }

    /// Full replacement update (PUT). `etag`, if provided, is sent as
    /// `If-Match` for optimistic concurrency; a stale ETag surfaces as
    /// `Error::PreconditionFailed`, not the generic `Error::Api`.
    ///
    /// Synchronous - verified against `cmd/incusd/storage_volumes.go`'s
    /// `storagePoolVolumePut`, which always returns
    /// `response.EmptySyncResponse`.
    pub async fn update_storage_volume(
        &self,
        pool_name: &str,
        volume_type: &str,
        volume_name: &str,
        new_definition: &serde_json::Value,
        etag: Option<&str>,
    ) -> Result<()> {
        let path = format!("/1.0/storage-pools/{pool_name}/volumes/{volume_type}/{volume_name}");
        self.request(Method::Put, &path, &[], Some(new_definition), etag)
            .await
            .map_err(|err| resource_error_or(err, volume_name))?;
        Ok(())
    }

    /// Same as [`Client::update_storage_volume`], but takes the `WithEtag`
    /// from a prior [`Client::get_storage_volume`] call directly instead of
    /// a bare `etag: Option<&str>` - `volume_type` and the volume's own
    /// `name` are derived from the fetched value; `pool_name` is still
    /// required explicitly since a volume's pool isn't part of its own
    /// returned object. See `instances::Client::update_instance_guarded`'s
    /// doc comment for why this exists alongside the raw-`etag` version.
    pub async fn update_storage_volume_guarded(
        &self,
        pool_name: &str,
        fetched: &WithEtag<StorageVolume>,
        new_definition: &serde_json::Value,
    ) -> Result<()> {
        self.update_storage_volume(
            pool_name,
            &fetched.value().volume_type,
            &fetched.value().name,
            new_definition,
            fetched.etag(),
        )
        .await
    }

    /// Synchronous - verified against `cmd/incusd/storage_volumes.go`'s
    /// `storagePoolVolumeDelete`, which always returns
    /// `response.EmptySyncResponse`.
    pub async fn delete_storage_volume(
        &self,
        pool_name: &str,
        volume_type: &str,
        volume_name: &str,
    ) -> Result<()> {
        let path = format!("/1.0/storage-pools/{pool_name}/volumes/{volume_type}/{volume_name}");
        self.request(Method::Delete, &path, &[], None, None)
            .await
            .map_err(|err| resource_error_or(err, volume_name))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
