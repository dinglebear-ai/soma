//! Process-wide single-flight coordination for inbound refresh grants.

use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use tokio::sync::Mutex;

static REFRESH_LOCKS: OnceLock<DashMap<String, Arc<Mutex<()>>>> = OnceLock::new();

/// Return the process-wide mutex for one stable authenticated subject.
///
/// Refresh-token predecessors for the same subject serialize around the
/// upstream provider refresh. This lets a concurrent retry wait for the first
/// request to durably rotate the token and publish its replay response instead
/// of racing the provider or spuriously returning `invalid_grant`.
pub(crate) fn lock(subject: &str) -> Arc<Mutex<()>> {
    REFRESH_LOCKS
        .get_or_init(DashMap::new)
        .entry(subject.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::lock;

    #[test]
    fn lock_is_shared_per_subject() {
        let left = lock("refresh-subject-lock-test");
        let right = lock("refresh-subject-lock-test");
        let other = lock("different-refresh-subject-lock-test");

        assert!(Arc::ptr_eq(&left, &right));
        assert!(!Arc::ptr_eq(&left, &other));
    }
}
