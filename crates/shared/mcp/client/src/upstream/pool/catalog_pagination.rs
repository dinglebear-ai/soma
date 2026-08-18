//! Bounded cursor pagination for upstream MCP catalog discovery.
//!
//! rmcp's `list_all_*` helpers follow upstream cursors without page, cursor,
//! byte, or total-deadline guards. Live gateway discovery must bound the work
//! before a buggy or hostile upstream can materialize an unbounded catalog.

use std::collections::HashSet;
use std::future::Future;
use std::io::Write;
use std::time::Duration;

use rmcp::RoleClient;
use rmcp::model::{PaginatedRequestParams, Prompt, Resource, Tool};
use rmcp::service::{Peer, ServiceError};
use serde::Serialize;
use thiserror::Error;
use tokio::time::Instant;

const MAX_CATALOG_PAGES: usize = 64;
const MAX_ITEMS_PER_PAGE: usize = 1_000;
const MAX_CURSOR_BYTES: usize = 8 * 1024;
const CATALOG_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub(super) enum CatalogPaginationError {
    #[error("upstream catalog request failed: {0}")]
    Service(#[from] ServiceError),
    #[error("upstream catalog pagination timed out after {deadline_ms}ms")]
    Deadline { deadline_ms: u128 },
    #[error("upstream catalog repeated a pagination cursor")]
    RepeatedCursor,
    #[error("upstream catalog exceeded the {limit}-page pagination limit")]
    PageLimit { limit: usize },
    #[error("upstream catalog page exceeded the {limit}-item limit")]
    PageItemLimit { limit: usize },
    #[error("upstream catalog cursor exceeded the {limit}-byte limit")]
    CursorLimit { limit: usize },
    #[error("upstream catalog reached {observed} serialized bytes, exceeding {limit} bytes")]
    ByteLimit { observed: usize, limit: usize },
}

struct ByteCounter(usize);

impl Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len());
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn serialized_len<T: Serialize>(value: &T) -> usize {
    let mut counter = ByteCounter(0);
    serde_json::to_writer(&mut counter, value).map_or(usize::MAX, |()| counter.0)
}

async fn collect_bounded<T, F, Fut>(
    byte_limit: usize,
    mut fetch: F,
) -> Result<Vec<T>, CatalogPaginationError>
where
    T: Serialize,
    F: FnMut(Option<PaginatedRequestParams>) -> Fut,
    Fut: Future<Output = Result<(Vec<T>, Option<String>), ServiceError>>,
{
    let started = Instant::now();
    let deadline_at = started + CATALOG_DISCOVERY_TIMEOUT;
    let mut items = Vec::new();
    let mut total_bytes = 0usize;
    let mut cursor = None;
    let mut seen_cursors = HashSet::new();

    for _page_index in 0..MAX_CATALOG_PAGES {
        let params = Some(PaginatedRequestParams::default().with_cursor(cursor.clone()));
        let (page, next_cursor) = tokio::time::timeout_at(deadline_at, fetch(params))
            .await
            .map_err(|_| CatalogPaginationError::Deadline {
                deadline_ms: CATALOG_DISCOVERY_TIMEOUT.as_millis(),
            })??;

        if page.len() > MAX_ITEMS_PER_PAGE {
            return Err(CatalogPaginationError::PageItemLimit {
                limit: MAX_ITEMS_PER_PAGE,
            });
        }
        for item in page {
            let item_bytes = serialized_len(&item);
            let observed = total_bytes.saturating_add(item_bytes);
            if observed > byte_limit {
                return Err(CatalogPaginationError::ByteLimit {
                    observed,
                    limit: byte_limit,
                });
            }
            total_bytes = observed;
            items.push(item);
        }

        let Some(next) = next_cursor else {
            return Ok(items);
        };
        if next.len() > MAX_CURSOR_BYTES {
            return Err(CatalogPaginationError::CursorLimit {
                limit: MAX_CURSOR_BYTES,
            });
        }
        let observed = total_bytes.saturating_add(next.len());
        if observed > byte_limit {
            return Err(CatalogPaginationError::ByteLimit {
                observed,
                limit: byte_limit,
            });
        }
        total_bytes = observed;
        if !seen_cursors.insert(next.clone()) {
            return Err(CatalogPaginationError::RepeatedCursor);
        }
        cursor = Some(next);
    }

    Err(CatalogPaginationError::PageLimit {
        limit: MAX_CATALOG_PAGES,
    })
}

fn empty_when_unsupported<T>(
    result: Result<Vec<T>, CatalogPaginationError>,
) -> Result<Vec<T>, CatalogPaginationError> {
    match result {
        Err(CatalogPaginationError::Service(ref error))
            if super::capability_is_absent(&error.to_string()) =>
        {
            Ok(Vec::new())
        }
        other => other,
    }
}

pub(super) async fn list_tools(
    peer: &Peer<RoleClient>,
    byte_limit: usize,
) -> Result<Vec<Tool>, CatalogPaginationError> {
    let result = collect_bounded(byte_limit, |params| async move {
        let response = peer.list_tools(params).await?;
        Ok((response.tools, response.next_cursor))
    })
    .await;
    empty_when_unsupported(result)
}

pub(super) async fn list_resources(
    peer: &Peer<RoleClient>,
    byte_limit: usize,
) -> Result<Vec<Resource>, CatalogPaginationError> {
    let result = collect_bounded(byte_limit, |params| async move {
        let response = peer.list_resources(params).await?;
        Ok((response.resources, response.next_cursor))
    })
    .await;

    empty_when_unsupported(result)
}

pub(super) async fn list_prompts(
    peer: &Peer<RoleClient>,
    byte_limit: usize,
) -> Result<Vec<Prompt>, CatalogPaginationError> {
    let result = collect_bounded(byte_limit, |params| async move {
        let response = peer.list_prompts(params).await?;
        Ok((response.prompts, response.next_cursor))
    })
    .await;

    empty_when_unsupported(result)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn repeated_cursor_is_rejected_after_two_requests() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let result = collect_bounded(1024, move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            async { Ok::<_, ServiceError>((Vec::<serde_json::Value>::new(), Some("again".into()))) }
        })
        .await;

        assert!(matches!(
            result,
            Err(CatalogPaginationError::RepeatedCursor)
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn serialized_catalog_bytes_are_bounded_during_collection() {
        let result = collect_bounded(16, |_| async {
            Ok::<_, ServiceError>((vec![json!({"payload": "this is too large"})], None))
        })
        .await;

        assert!(matches!(
            result,
            Err(CatalogPaginationError::ByteLimit { limit: 16, .. })
        ));
    }

    #[tokio::test]
    async fn oversized_cursor_is_rejected_before_retention() {
        let result = collect_bounded::<serde_json::Value, _, _>(usize::MAX, |_| async {
            Ok::<_, ServiceError>((Vec::new(), Some("x".repeat(MAX_CURSOR_BYTES + 1))))
        })
        .await;

        assert!(matches!(
            result,
            Err(CatalogPaginationError::CursorLimit { .. })
        ));
    }

    #[test]
    fn tools_method_not_found_is_a_toolless_catalog() {
        let result: Result<Vec<Tool>, CatalogPaginationError> =
            Err(CatalogPaginationError::Service(ServiceError::McpError(
                rmcp::model::ErrorData::method_not_found::<rmcp::model::CallToolRequestMethod>(),
            )));
        let result = empty_when_unsupported(result);
        assert!(matches!(result, Ok(tools) if tools.is_empty()));
    }
}
