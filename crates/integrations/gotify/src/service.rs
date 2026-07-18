use serde_json::{json, Value};

use crate::error::Result;
use crate::GotifyClient;

/// Business-logic facade over [`GotifyClient`]: the same endpoints, plus
/// client-side filtering and pagination shaping that any embedder is likely
/// to want (Gotify's own API has no server-side text search, and its
/// `applications`/`clients` listings have no filter parameter at all).
///
/// Consumers embedding this crate (CLI commands, MCP tools, HTTP handlers)
/// should depend on `GotifyService`, not [`GotifyClient`] directly — it is
/// the stable seam for adding cross-cutting behavior without touching every
/// call site.
///
/// Deliberately **not** included here: request counters, uptime/status
/// reporting, or a destructive-action confirmation gate. Those are
/// server/product policy (how *your* MCP tool or CLI chooses to expose
/// `delete_*`), not something this crate should bake in — same reasoning as
/// `unifi`'s `AuthScope` being metadata only, not enforced by the crate.
#[derive(Clone)]
pub struct GotifyService {
    client: GotifyClient,
}

impl GotifyService {
    /// Wraps an already-built [`GotifyClient`].
    pub fn new(client: GotifyClient) -> Self {
        Self { client }
    }

    // ── unauthenticated ───────────────────────────────────────────────────────

    /// Server health check.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn health(&self) -> Result<Value> {
        self.client.health().await
    }

    /// Server version.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn version(&self) -> Result<Value> {
        self.client.version().await
    }

    // ── current user ─────────────────────────────────────────────────────────

    /// Authenticated user info.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn me(&self) -> Result<Value> {
        self.client.me().await
    }

    // ── messages ──────────────────────────────────────────────────────────────

    /// Lists messages with client-side text search and offset pagination on
    /// top of [`GotifyClient::messages`]'s server-paginated result.
    ///
    /// `limit` is capped at 200 and defaults to 50. `query`, when given,
    /// keeps only messages whose `message` or `title` contains it
    /// (case-insensitive). `offset` skips that many matching results before
    /// the page starts.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn messages(
        &self,
        app_id: Option<i64>,
        limit: Option<i64>,
        since: Option<i64>,
        offset: Option<i64>,
        query: Option<&str>,
    ) -> Result<Value> {
        let limit = limit.unwrap_or(50).min(200);
        let result = self.client.messages(app_id, Some(limit), since).await?;

        let mut messages: Vec<Value> = result
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .or_else(|| result.as_array().cloned())
            .unwrap_or_default();
        let total_before_filter = messages.len();

        if let Some(query) = query {
            let query = query.to_lowercase();
            messages.retain(|message| {
                let body = message["message"].as_str().unwrap_or("").to_lowercase();
                let title = message["title"].as_str().unwrap_or("").to_lowercase();
                body.contains(&query) || title.contains(&query)
            });
        }

        let offset = offset.unwrap_or(0).max(0) as usize;
        messages = if offset < messages.len() {
            messages[offset..].to_vec()
        } else {
            Vec::new()
        };

        let total = if query.is_some() {
            messages.len() as i64
        } else {
            result["paging"]["size"]
                .as_i64()
                .unwrap_or(total_before_filter as i64)
        };
        let has_more = messages.len() as i64 >= limit;
        let next_offset = offset as i64 + messages.len() as i64;

        Ok(json!({
            "messages": messages,
            "total": total,
            "limit": limit,
            "offset": offset,
            "has_more": has_more,
            "next_offset": next_offset,
        }))
    }

    /// Sends a notification.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn send_message(
        &self,
        message: &str,
        title: Option<&str>,
        priority: Option<i64>,
        extras: Option<Value>,
    ) -> Result<Value> {
        self.client
            .send_message(message, title, priority, extras)
            .await
    }

    /// Deletes one message.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn delete_message(&self, id: i64) -> Result<Value> {
        self.client.delete_message(id).await
    }

    /// Deletes every message.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn delete_all_messages(&self) -> Result<Value> {
        self.client.delete_all_messages().await
    }

    // ── applications ──────────────────────────────────────────────────────────

    /// Lists applications, optionally filtered by a case-insensitive
    /// substring match on `name` — Gotify's own API has no filter parameter
    /// for this listing.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn applications(&self, name_filter: Option<&str>) -> Result<Value> {
        let result = self.client.applications().await?;
        let Some(filter) = name_filter else {
            return Ok(result);
        };

        let filter_lower = filter.to_lowercase();
        let filtered: Vec<Value> = result
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|app| {
                app["name"]
                    .as_str()
                    .is_some_and(|name| name.to_lowercase().contains(&filter_lower))
            })
            .collect();

        Ok(json!({
            "applications": filtered,
            "total": filtered.len(),
            "filter_name": filter,
        }))
    }

    /// Creates an application.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn create_application(
        &self,
        name: &str,
        description: Option<&str>,
        default_priority: Option<i64>,
    ) -> Result<Value> {
        self.client
            .create_application(name, description, default_priority)
            .await
    }

    /// Updates an application.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn update_application(
        &self,
        app_id: i64,
        name: Option<&str>,
        description: Option<&str>,
        default_priority: Option<i64>,
    ) -> Result<Value> {
        self.client
            .update_application(app_id, name, description, default_priority)
            .await
    }

    /// Deletes an application.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn delete_application(&self, app_id: i64) -> Result<Value> {
        self.client.delete_application(app_id).await
    }

    // ── clients ───────────────────────────────────────────────────────────────

    /// Lists clients.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn clients(&self) -> Result<Value> {
        self.client.clients().await
    }

    /// Creates a client.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn create_client(&self, name: &str) -> Result<Value> {
        self.client.create_client(name).await
    }

    /// Deletes a client.
    ///
    /// # Errors
    /// See [`crate::GotifyError`] for the failure cases this can return.
    pub async fn delete_client(&self, client_id: i64) -> Result<Value> {
        self.client.delete_client(client_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GotifyConfig;

    fn service() -> GotifyService {
        let client = GotifyClient::new(&GotifyConfig {
            url: "https://gotify.local".to_string(),
            ..GotifyConfig::default()
        })
        .unwrap();
        GotifyService::new(client)
    }

    #[test]
    fn service_can_be_constructed_from_a_client() {
        let _ = service();
    }
}
