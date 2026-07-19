//! Image CRUD.

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::operations::{operation_from_envelope, Operation};
use crate::transport::{
    resource_error_or, sync_metadata, Client, Method, RecursionQuery, WithEtag,
};

#[derive(Debug, Clone, Deserialize)]
pub struct Image {
    pub fingerprint: String,
    pub public: bool,
    pub filename: String,
    pub size: i64,
    pub architecture: String,
    pub created_at: String,
    pub uploaded_at: String,
    #[serde(default)]
    pub properties: serde_json::Value,
}

impl Client {
    /// `recursion = true` fetches every image's full object in one call;
    /// `recursion = false` returns lightweight fingerprint/URL references.
    pub async fn list_images(&self, recursion: bool) -> Result<Vec<serde_json::Value>> {
        let recursion_query = RecursionQuery::new(recursion);
        let envelope = self
            .request(
                Method::Get,
                "/1.0/images",
                &recursion_query.as_query(),
                None,
                None,
            )
            .await?;
        Ok(serde_json::from_value(sync_metadata(envelope, "list")?)?)
    }

    /// Fetches one image by fingerprint, along with its ETag for use as a
    /// later `If-Match` precondition.
    pub async fn get_image(&self, fingerprint: &str) -> Result<WithEtag<Image>> {
        let path = format!("/1.0/images/{fingerprint}");
        let envelope = self
            .request(Method::Get, &path, &[], None, None)
            .await
            .map_err(|err| resource_error_or(err, fingerprint))?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, etag } => Ok(WithEtag {
                value: serde_json::from_value(metadata)?,
                etag,
            }),
            other => Err(Error::InvalidResponse(format!(
                "expected a sync image response, got {other:?}"
            ))),
        }
    }

    /// Always async: image import/creation is documented as a long-running
    /// operation (fetching/unpacking a source, which can be a remote URL or
    /// a large upload).
    pub async fn create_image(&self, params: &serde_json::Value) -> Result<Operation> {
        let envelope = self
            .request(Method::Post, "/1.0/images", &[], Some(params), None)
            .await?;
        operation_from_envelope(envelope)
    }

    /// Full replacement update (PUT). `etag`, if provided, is sent as
    /// `If-Match` for optimistic concurrency; a stale ETag surfaces as
    /// `Error::PreconditionFailed`, not the generic `Error::Api`.
    pub async fn update_image(
        &self,
        fingerprint: &str,
        new_definition: &serde_json::Value,
        etag: Option<&str>,
    ) -> Result<Operation> {
        let path = format!("/1.0/images/{fingerprint}");
        let envelope = self
            .request(Method::Put, &path, &[], Some(new_definition), etag)
            .await
            .map_err(|err| resource_error_or(err, fingerprint))?;
        operation_from_envelope(envelope)
    }

    /// Same as [`Client::update_image`], but takes the `WithEtag` from a
    /// prior [`Client::get_image`] call directly instead of a bare
    /// `etag: Option<&str>` - see `instances::Client::update_instance_guarded`'s
    /// doc comment for why this exists alongside the raw-`etag` version.
    pub async fn update_image_guarded(
        &self,
        fetched: &WithEtag<Image>,
        new_definition: &serde_json::Value,
    ) -> Result<Operation> {
        self.update_image(&fetched.value().fingerprint, new_definition, fetched.etag())
            .await
    }

    pub async fn delete_image(&self, fingerprint: &str) -> Result<Operation> {
        let path = format!("/1.0/images/{fingerprint}");
        let envelope = self
            .request(Method::Delete, &path, &[], None, None)
            .await
            .map_err(|err| resource_error_or(err, fingerprint))?;
        operation_from_envelope(envelope)
    }
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod tests;
