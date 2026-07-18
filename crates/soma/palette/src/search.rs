//! In-memory search over the mapped Palette catalog.
//!
//! Search is intentionally simple: a case-insensitive substring match over
//! `id`, `title`, `category`, and `description`, ranked by which field
//! matched first (`id`, then `title`, then `category`, then `description`),
//! with original catalog order as the tiebreak. Callers that need fuzzier
//! matching should do it client-side over the full catalog — this exists to
//! keep `GET /v1/palette/search?q=` cheap and predictable.

use crate::dto::LauncherCatalogEntry;

const DEFAULT_LIMIT: usize = 50;

#[must_use]
pub fn search_entries(
    entries: &[LauncherCatalogEntry],
    query: &str,
    limit: Option<usize>,
) -> Vec<LauncherCatalogEntry> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).max(1);
    let query = query.trim();
    if query.is_empty() {
        return entries.iter().take(limit).cloned().collect();
    }
    let needle = query.to_ascii_lowercase();

    let mut scored: Vec<(u8, usize, &LauncherCatalogEntry)> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| match_rank(entry, &needle).map(|rank| (rank, index, entry)))
        .collect();
    scored.sort_by_key(|(rank, index, _)| (*rank, *index));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, _, entry)| entry.clone())
        .collect()
}

/// Lower rank sorts first. `None` means no match at all.
fn match_rank(entry: &LauncherCatalogEntry, needle: &str) -> Option<u8> {
    if entry.id.to_ascii_lowercase().contains(needle) {
        return Some(0);
    }
    if entry.title.to_ascii_lowercase().contains(needle) {
        return Some(1);
    }
    if entry
        .category
        .as_deref()
        .is_some_and(|category| category.to_ascii_lowercase().contains(needle))
    {
        return Some(2);
    }
    if entry.description.to_ascii_lowercase().contains(needle) {
        return Some(3);
    }
    None
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
