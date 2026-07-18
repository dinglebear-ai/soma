use super::search_entries;
use crate::dto::LauncherCatalogEntry;

fn entry(id: &str, title: &str, description: &str, category: Option<&str>) -> LauncherCatalogEntry {
    LauncherCatalogEntry {
        id: id.to_string(),
        provider: "test".to_string(),
        title: title.to_string(),
        description: description.to_string(),
        category: category.map(ToOwned::to_owned),
        icon: None,
        tone: None,
        arg_mode: None,
        result_view: None,
        destructive: false,
        requires_admin: false,
    }
}

fn fixture() -> Vec<LauncherCatalogEntry> {
    vec![
        entry(
            "send_alert",
            "Send Alert",
            "Push a notification",
            Some("notify"),
        ),
        entry(
            "list_containers",
            "List Containers",
            "List docker containers",
            Some("docker"),
        ),
        entry(
            "restart_service",
            "Restart Service",
            "restart a systemd unit, notify on failure",
            None,
        ),
    ]
}

#[test]
fn empty_query_returns_entries_up_to_limit() {
    let entries = fixture();
    let results = search_entries(&entries, "", Some(2));
    assert_eq!(results.len(), 2);
}

#[test]
fn matches_are_case_insensitive() {
    let entries = fixture();
    let results = search_entries(&entries, "ALERT", None);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "send_alert");
}

#[test]
fn id_matches_rank_above_description_only_matches() {
    let entries = fixture();
    let results = search_entries(&entries, "notify", None);
    // "send_alert" matches via category; "restart_service" matches via
    // description only. Category ranks above description.
    assert_eq!(results.first().map(|e| e.id.as_str()), Some("send_alert"));
}

#[test]
fn no_match_returns_empty() {
    let entries = fixture();
    assert!(search_entries(&entries, "nonexistent-xyz", None).is_empty());
}

#[test]
fn respects_limit() {
    let entries = fixture();
    let results = search_entries(&entries, "", Some(1));
    assert_eq!(results.len(), 1);
}

#[test]
fn zero_limit_clamps_to_one_rather_than_returning_empty() {
    let entries = fixture();
    let results = search_entries(&entries, "", Some(0));
    assert_eq!(results.len(), 1);
}

#[test]
fn whitespace_only_query_behaves_like_empty_query() {
    let entries = fixture();
    let results = search_entries(&entries, "   ", None);
    assert_eq!(results.len(), entries.len());
}
