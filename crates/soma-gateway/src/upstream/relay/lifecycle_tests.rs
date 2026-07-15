use std::time::{Duration, Instant};

use super::*;
use crate::upstream::relay::{
    RelayCache, RelayCacheKey, RelayCapabilities, RelayConnection, RelaySessionMint,
};

#[test]
fn lifecycle_sweep_collects_evicted_transports_for_off_lock_shutdown() {
    let mint = RelaySessionMint::new();
    let now = Instant::now();
    let key = RelayCacheKey {
        upstream: "u".to_owned(),
        session_id: mint.mint(),
        subject: None,
    };
    let mut cache = RelayCache::new(Duration::from_millis(1), 8);
    cache.complete_connect(RelayConnection {
        key: key.clone(),
        created_at: now,
        last_used: now,
        alive: true,
        capabilities: RelayCapabilities::default(),
    });

    let shutdowns = sweep_and_collect_shutdowns(&mut cache, now + Duration::from_secs(1));

    assert_eq!(shutdowns.len(), 1);
    assert_eq!(shutdowns[0].key, key);
    assert!(cache.is_empty());
}
