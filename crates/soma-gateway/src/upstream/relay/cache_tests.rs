use std::time::{Duration, Instant};

use super::*;
use crate::upstream::relay::RelaySessionMint;

fn key(upstream: &str, session_id: RelaySessionId, subject: Option<&str>) -> RelayCacheKey {
    RelayCacheKey {
        upstream: upstream.to_owned(),
        session_id,
        subject: subject.map(str::to_owned),
    }
}

fn connection(key: RelayCacheKey, now: Instant) -> RelayConnection {
    RelayConnection {
        key,
        created_at: now,
        last_used: now,
        alive: true,
        capabilities: RelayCapabilities::default(),
    }
}

#[test]
fn relay_key_is_upstream_session_and_subject() {
    let mint = RelaySessionMint::new();
    let session = mint.mint();

    assert_ne!(key("u", session, Some("a")), key("u", session, Some("b")));
    assert_ne!(
        key("u", session, Some("a")),
        key("u", mint.mint(), Some("a"))
    );
}

#[test]
fn same_key_burst_single_flights() {
    let mint = RelaySessionMint::new();
    let key = key("u", mint.mint(), Some("s"));
    let now = Instant::now();
    let mut cache = RelayCache::new(Duration::from_secs(60), 8);

    assert_eq!(
        cache.begin_connect(key.clone(), now),
        RelayConnectSlot::Leader
    );
    assert_eq!(cache.begin_connect(key, now), RelayConnectSlot::Waiter);
    assert_eq!(cache.lock_count(), 1);
}

#[test]
fn leader_failure_and_cancellation_release_lock() {
    let mint = RelaySessionMint::new();
    let key = key("u", mint.mint(), None);
    let now = Instant::now();
    let mut cache = RelayCache::new(Duration::from_secs(60), 8);

    assert_eq!(
        cache.begin_connect(key.clone(), now),
        RelayConnectSlot::Leader
    );
    cache.leader_cancelled(&key);
    assert_eq!(cache.lock_count(), 0);
    assert_eq!(
        cache.begin_connect(key.clone(), now),
        RelayConnectSlot::Leader
    );
    cache.connect_failed(&key);
    assert_eq!(cache.lock_count(), 0);
}

#[test]
fn ttl_lru_and_dead_eviction_queue_shutdown_off_lock() {
    let mint = RelaySessionMint::new();
    let now = Instant::now();
    let mut cache = RelayCache::new(Duration::from_secs(1), 1);
    let first = key("u", mint.mint(), None);
    let second = key("u", mint.mint(), None);

    cache.complete_connect(connection(first.clone(), now));
    cache.complete_connect(connection(second.clone(), now + Duration::from_millis(1)));
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.take_pending_shutdown()[0].key, first);

    cache.mark_dead(&second);
    cache.sweep(now + Duration::from_secs(2));
    assert!(cache.is_empty());
    assert_eq!(cache.take_pending_shutdown()[0].key, second);
}

#[test]
fn waiter_cancellation_does_not_clear_leader_lock() {
    let mint = RelaySessionMint::new();
    let key = key("u", mint.mint(), None);
    let now = Instant::now();
    let mut cache = RelayCache::new(Duration::from_secs(60), 8);

    assert_eq!(
        cache.begin_connect(key.clone(), now),
        RelayConnectSlot::Leader
    );
    cache.waiter_cancelled(&key);

    assert_eq!(cache.lock_count(), 1);
}
