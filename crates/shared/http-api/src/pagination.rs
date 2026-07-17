//! Generic pagination query and response DTOs.
//!
//! Not yet wired into any Soma route — no current Soma list action needs
//! pagination — but declared here per plan section 3.11 so the first product
//! route that does need it has a shared shape to reach for instead of
//! inventing another one-off `limit`/`offset` pair.

use serde::{Deserialize, Serialize};

fn default_limit() -> usize {
    50
}

/// Query parameters for a paginated list route: `?limit=&offset=`.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PageParams {
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub offset: usize,
}

impl Default for PageParams {
    fn default() -> Self {
        Self {
            limit: default_limit(),
            offset: 0,
        }
    }
}

impl PageParams {
    /// Clamp `limit` to `max`. This is opt-in — the type itself does not
    /// enforce a bound, so callers that build a `Page` from client-supplied
    /// `PageParams` must call this (or otherwise validate `limit`) before
    /// passing the params to a query; nothing at the type level prevents
    /// skipping this step.
    #[must_use]
    pub fn clamped(mut self, max: usize) -> Self {
        self.limit = self.limit.min(max);
        self
    }
}

/// A single page of `T` plus enough metadata to fetch the next one.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub limit: usize,
    pub offset: usize,
    /// Total item count across all pages, when the source can report it
    /// cheaply. `None` when computing it would require a second query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, params: PageParams, total: Option<usize>) -> Self {
        Self {
            items,
            limit: params.limit,
            offset: params.offset,
            total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_params_default_limit_is_fifty() {
        assert_eq!(
            PageParams::default(),
            PageParams {
                limit: 50,
                offset: 0
            }
        );
    }

    #[test]
    fn page_params_clamped_never_exceeds_max() {
        let params = PageParams {
            limit: 500,
            offset: 0,
        }
        .clamped(100);
        assert_eq!(params.limit, 100);
    }

    #[test]
    fn page_carries_params_and_total() {
        let page = Page::new(
            vec!["a", "b"],
            PageParams {
                limit: 2,
                offset: 4,
            },
            Some(10),
        );
        assert_eq!(page.items, vec!["a", "b"]);
        assert_eq!(page.limit, 2);
        assert_eq!(page.offset, 4);
        assert_eq!(page.total, Some(10));
    }

    #[test]
    fn page_omits_total_when_unknown() {
        let page = Page::new(vec!["a"], PageParams::default(), None);
        assert_eq!(
            serde_json::to_value(&page).unwrap(),
            serde_json::json!({ "items": ["a"], "limit": 50, "offset": 0 })
        );
    }
}
