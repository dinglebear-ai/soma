//! Project CRUD.

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::transport::{
    resource_error_or, sync_metadata, Client, Method, RecursionQuery, WithEtag,
};

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

impl Client {
    pub async fn list_projects(&self, recursion: bool) -> Result<Vec<serde_json::Value>> {
        let recursion_query = RecursionQuery::new(recursion);
        let envelope = self
            .request(
                Method::Get,
                "/1.0/projects",
                &recursion_query.as_query(),
                None,
                None,
            )
            .await?;
        Ok(serde_json::from_value(sync_metadata(envelope, "list")?)?)
    }

    /// Fetches one project by name, along with its ETag for use as a later
    /// `If-Match` precondition.
    pub async fn get_project(&self, name: &str) -> Result<WithEtag<Project>> {
        let path = format!("/1.0/projects/{name}");
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
                "expected a sync project response, got {other:?}"
            ))),
        }
    }

    /// Creates a project. Synchronous: the project exists by the time this
    /// returns, with no operation to wait on. Verified against
    /// `cmd/incusd/api_project.go`'s `projectsPost` on the `lxc/incus`
    /// `main` branch, which always returns `response.SyncResponseLocation`.
    pub async fn create_project(&self, params: &serde_json::Value) -> Result<()> {
        self.request(Method::Post, "/1.0/projects", &[], Some(params), None)
            .await?;
        Ok(())
    }

    /// Full replacement update (PUT). `etag`, if provided, is sent as
    /// `If-Match` for optimistic concurrency; a stale ETag surfaces as
    /// `Error::PreconditionFailed`, not the generic `Error::Api`.
    ///
    /// Synchronous, like [`Client::create_project`] - verified against
    /// `cmd/incusd/api_project.go`'s `projectChange`, which always returns
    /// `response.EmptySyncResponse`.
    pub async fn update_project(
        &self,
        name: &str,
        new_definition: &serde_json::Value,
        etag: Option<&str>,
    ) -> Result<()> {
        let path = format!("/1.0/projects/{name}");
        self.request(Method::Put, &path, &[], Some(new_definition), etag)
            .await
            .map_err(|err| resource_error_or(err, name))?;
        Ok(())
    }

    /// Same as [`Client::update_project`], but takes the `WithEtag` from a
    /// prior [`Client::get_project`] call directly instead of a bare
    /// `etag: Option<&str>` - see `instances::Client::update_instance_guarded`'s
    /// doc comment for why this exists alongside the raw-`etag` version.
    pub async fn update_project_guarded(
        &self,
        fetched: &WithEtag<Project>,
        new_definition: &serde_json::Value,
    ) -> Result<()> {
        self.update_project(&fetched.value().name, new_definition, fetched.etag())
            .await
    }

    /// Synchronous - verified against `cmd/incusd/api_project.go`'s
    /// `projectDelete`, which always returns `response.EmptySyncResponse`.
    pub async fn delete_project(&self, name: &str) -> Result<()> {
        let path = format!("/1.0/projects/{name}");
        self.request(Method::Delete, &path, &[], None, None)
            .await
            .map_err(|err| resource_error_or(err, name))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "projects_tests.rs"]
mod tests;
