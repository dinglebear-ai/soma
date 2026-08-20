//! Upstream MCP notification subscriptions and normalized pool events.

use std::collections::BTreeMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use rmcp::model::{ErrorCode, ProtocolVersion, ServerNotification, SubscriptionFilter};
use rmcp::service::ServiceError;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::UpstreamPool;

const NOTIFICATION_EVENT_CAPACITY: usize = 1024;
const MAX_RESOURCE_UPDATE_URI_BYTES: usize = 64 * 1024;
const SUBSCRIPTION_ESTABLISH_TIMEOUT: Duration = Duration::from_secs(15);
const SUBSCRIPTION_STABLE_INTERVAL: Duration = Duration::from_secs(30);
const SUBSCRIPTION_MAX_BACKOFF: Duration = Duration::from_secs(60);

mod listen;
mod refresh;

pub(super) struct SubscriptionOwner {
    generation: Arc<CancellationToken>,
}

impl SubscriptionOwner {
    fn new(generation: Arc<CancellationToken>) -> Self {
        Self { generation }
    }
}

impl Drop for SubscriptionOwner {
    fn drop(&mut self) {
        self.generation.cancel();
    }
}

pub(super) type SubscriptionRegistry = std::sync::RwLock<BTreeMap<String, SubscriptionOwner>>;

pub(super) struct SubscriptionGenerationGuard {
    registry: Weak<SubscriptionRegistry>,
    upstream: String,
    generation: Arc<CancellationToken>,
    armed: bool,
}

impl SubscriptionGenerationGuard {
    fn new(
        registry: &Arc<SubscriptionRegistry>,
        upstream: &str,
        generation: Arc<CancellationToken>,
    ) -> Self {
        Self {
            registry: Arc::downgrade(registry),
            upstream: upstream.to_owned(),
            generation,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SubscriptionGenerationGuard {
    fn drop(&mut self) {
        if self.armed {
            retire_generation_if_current_weak(&self.registry, &self.upstream, &self.generation);
        }
    }
}

#[cfg(test)]
fn generation_is_current(
    registry: &SubscriptionRegistry,
    upstream: &str,
    generation: &Arc<CancellationToken>,
) -> bool {
    registry
        .read()
        .expect("upstream subscription lock poisoned")
        .get(upstream)
        .is_some_and(|owner| Arc::ptr_eq(&owner.generation, generation))
}

fn retire_generation_if_current(
    registry: &SubscriptionRegistry,
    upstream: &str,
    generation: &Arc<CancellationToken>,
) {
    let mut generations = registry
        .write()
        .expect("upstream subscription lock poisoned");
    if generations
        .get(upstream)
        .is_some_and(|owner| Arc::ptr_eq(&owner.generation, generation))
    {
        generations.remove(upstream);
    }
}

fn retire_generation_if_current_weak(
    registry: &Weak<SubscriptionRegistry>,
    upstream: &str,
    generation: &Arc<CancellationToken>,
) {
    if let Some(registry) = registry.upgrade() {
        retire_generation_if_current(&registry, upstream, generation);
    }
}

fn publish_event_if_current(
    registry: &Weak<SubscriptionRegistry>,
    notification_tx: &broadcast::Sender<UpstreamNotificationEvent>,
    upstream: &str,
    generation: &Arc<CancellationToken>,
    event: UpstreamNotificationEvent,
) -> bool {
    let Some(registry) = registry.upgrade() else {
        return false;
    };
    let generations = registry
        .read()
        .expect("upstream subscription lock poisoned");
    if !generations
        .get(upstream)
        .is_some_and(|owner| Arc::ptr_eq(&owner.generation, generation))
    {
        return false;
    }
    drop(notification_tx.send(event));
    true
}

/// A notification observed on an upstream MCP connection and normalized for
/// transport-neutral gateway consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UpstreamNotificationEvent {
    ToolListChanged { upstream: String },
    PromptListChanged { upstream: String },
    ResourceListChanged { upstream: String },
    ResourceUpdated { upstream: String, uri: String },
}

fn resource_updated_event(upstream: &str, uri: String) -> Option<UpstreamNotificationEvent> {
    if uri.len() > MAX_RESOURCE_UPDATE_URI_BYTES {
        tracing::warn!(
            upstream,
            uri_bytes = uri.len(),
            limit = MAX_RESOURCE_UPDATE_URI_BYTES,
            "dropping oversized upstream resource_updated notification"
        );
        return None;
    }
    Some(UpstreamNotificationEvent::ResourceUpdated {
        upstream: upstream.to_owned(),
        uri,
    })
}

pub(super) fn notification_channel() -> (
    broadcast::Sender<UpstreamNotificationEvent>,
    broadcast::Receiver<UpstreamNotificationEvent>,
) {
    broadcast::channel(NOTIFICATION_EVENT_CAPACITY)
}

pub(super) fn subscription_listen_supported_protocol(version: &ProtocolVersion) -> bool {
    version == &ProtocolVersion::V_2026_07_28
}

pub(super) fn terminal_subscription_listen_error(error: &ServiceError, retry_attempt: u32) -> bool {
    match error {
        ServiceError::McpError(error) if error.code == ErrorCode::METHOD_NOT_FOUND => true,
        ServiceError::McpError(error) if error.code == ErrorCode::INVALID_PARAMS => {
            retry_attempt > 0
        }
        ServiceError::McpError(error) if error.code == ErrorCode::INTERNAL_ERROR => error
            .message
            .to_ascii_lowercase()
            .contains("subscription limit reached"),
        _ => false,
    }
}

pub(super) fn subscription_retry_delay(upstream: &str, attempt: u32) -> Duration {
    let exponent = attempt.min(6);
    let base_ms = 1_000_u64
        .saturating_mul(1_u64 << exponent)
        .min(SUBSCRIPTION_MAX_BACKOFF.as_millis() as u64);
    let seed = upstream
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
        ^ u64::from(attempt);
    let jitter_permille = 800_u64 + (seed % 401);
    Duration::from_millis(base_ms.saturating_mul(jitter_permille) / 1_000)
}

pub(super) fn next_subscription_retry_attempt(attempt: u32, established_for: Duration) -> u32 {
    if established_for >= SUBSCRIPTION_STABLE_INTERVAL {
        0
    } else {
        attempt.saturating_add(1)
    }
}

fn subscription_filter_is_empty(filter: &SubscriptionFilter) -> bool {
    filter.tools_list_changed != Some(true)
        && filter.prompts_list_changed != Some(true)
        && filter.resources_list_changed != Some(true)
        && filter
            .resource_subscriptions
            .as_ref()
            .is_none_or(Vec::is_empty)
}

impl UpstreamPool {
    /// Subscribe to normalized upstream notifications. Slow receivers cannot
    /// block the upstream MCP connection.
    pub fn subscribe_notifications(&self) -> broadcast::Receiver<UpstreamNotificationEvent> {
        self.notification_tx.subscribe()
    }

    pub(super) fn cancel_upstream_subscription(&self, upstream: &str) {
        self.subscription_tasks
            .write()
            .expect("upstream subscription lock poisoned")
            .remove(upstream);
    }

    fn begin_subscription_generation(&self, upstream: &str) -> Arc<CancellationToken> {
        let generation = Arc::new(CancellationToken::new());
        self.subscription_tasks
            .write()
            .expect("upstream subscription lock poisoned")
            .insert(
                upstream.to_owned(),
                SubscriptionOwner::new(Arc::clone(&generation)),
            );
        generation
    }

    #[cfg(test)]
    fn subscription_generation_is_current(
        &self,
        upstream: &str,
        generation: &Arc<CancellationToken>,
    ) -> bool {
        generation_is_current(&self.subscription_tasks, upstream, generation)
    }

    fn retire_subscription_generation_if_current(
        &self,
        upstream: &str,
        generation: &Arc<CancellationToken>,
    ) {
        retire_generation_if_current(&self.subscription_tasks, upstream, generation);
    }
}

#[cfg(test)]
#[path = "notifications_tests.rs"]
mod tests;
