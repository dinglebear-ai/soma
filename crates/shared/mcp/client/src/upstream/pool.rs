use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use serde_json::Value;

use crate::config::UpstreamConfig;
use crate::process::guard::SpawnGuard;
use crate::upstream::http_client::{decide_http_transport, transport_kind_for_decision};
use crate::upstream::{
    ResponseCaps, ToolDescriptor, TransportKind, UpstreamError, UpstreamHealth, UpstreamSnapshot,
};

pub mod connect_stdio;
pub mod discovery;
pub mod health;
pub mod live;
pub mod prompts;
pub mod resources;
#[cfg(feature = "oauth")]
pub mod subject;
pub mod tools;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolOptions {
    pub response_caps: ResponseCaps,
    pub discovery_concurrency: usize,
}

impl Default for PoolOptions {
    fn default() -> Self {
        Self {
            response_caps: ResponseCaps::default(),
            discovery_concurrency: 8,
        }
    }
}

impl PoolOptions {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.discovery_concurrency = self.discovery_concurrency.max(1);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub upstream: String,
    pub tool: String,
    pub params: Value,
}

#[derive(Debug, Clone)]
pub struct InProcessUpstream {
    snapshot: UpstreamSnapshot,
    tool_results: BTreeMap<String, Value>,
}

impl InProcessUpstream {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            snapshot: UpstreamSnapshot::empty(name, TransportKind::InProcess),
            tool_results: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_tool(mut self, tool: ToolDescriptor, result: Value) -> Self {
        self.tool_results.insert(tool.name.clone(), result);
        self.snapshot.tools.push(tool);
        self
    }

    #[must_use]
    pub fn with_snapshot(mut self, snapshot: UpstreamSnapshot) -> Self {
        self.snapshot = snapshot;
        self
    }

    fn call_tool(&self, call: &ToolCall) -> Result<Value, UpstreamError> {
        if !call.params.is_object() {
            return Err(UpstreamError::ParamsMustBeObject);
        }
        self.tool_results
            .get(&call.tool)
            .cloned()
            .ok_or_else(|| UpstreamError::NotExposed {
                upstream: call.upstream.clone(),
                item: call.tool.clone(),
            })
    }
}

struct PoolEntry {
    config: UpstreamConfig,
    snapshot: UpstreamSnapshot,
    in_process: Option<InProcessUpstream>,
    live: Option<Arc<live::LiveUpstream>>,
}

#[cfg(feature = "oauth")]
struct SubjectPoolEntry {
    snapshot: UpstreamSnapshot,
    live: Arc<live::LiveUpstream>,
}

#[derive(Clone)]
pub struct UpstreamPool {
    entries: Arc<RwLock<BTreeMap<String, PoolEntry>>>,
    #[cfg(feature = "oauth")]
    subject_entries: Arc<RwLock<BTreeMap<(String, String), SubjectPoolEntry>>>,
    #[cfg(feature = "oauth")]
    oauth_provider: Arc<RwLock<Option<Arc<dyn crate::oauth::UpstreamOAuthProvider>>>>,
    options: PoolOptions,
}

impl Default for UpstreamPool {
    fn default() -> Self {
        Self::new(PoolOptions::default())
    }
}

impl UpstreamPool {
    #[must_use]
    pub fn new(options: PoolOptions) -> Self {
        Self {
            entries: Arc::new(RwLock::new(BTreeMap::new())),
            #[cfg(feature = "oauth")]
            subject_entries: Arc::new(RwLock::new(BTreeMap::new())),
            #[cfg(feature = "oauth")]
            oauth_provider: Arc::new(RwLock::new(None)),
            options: options.normalized(),
        }
    }

    #[must_use]
    pub fn response_caps(&self) -> &ResponseCaps {
        &self.options.response_caps
    }

    #[must_use]
    pub fn discovery_concurrency(&self) -> usize {
        self.options.discovery_concurrency
    }

    pub fn register_config(&self, config: UpstreamConfig) -> Result<(), UpstreamError> {
        let transport = transport_for_config(&config);
        let health = if config.enabled {
            health_for_config(config.name.as_str(), transport)
        } else {
            UpstreamHealth::Disabled
        };
        let mut snapshot = UpstreamSnapshot::empty(config.name.clone(), transport);
        snapshot.health = health;
        let entry = PoolEntry {
            config: config.clone(),
            snapshot,
            in_process: None,
            live: None,
        };
        self.entries
            .write()
            .expect("upstream pool lock poisoned")
            .insert(config.name.clone(), entry);
        Ok(())
    }

    pub fn register_in_process(
        &self,
        config: UpstreamConfig,
        upstream: InProcessUpstream,
    ) -> Result<(), UpstreamError> {
        let mut snapshot = upstream.snapshot.clone();
        snapshot.name = config.name.clone();
        snapshot.transport = TransportKind::InProcess;
        snapshot.health = if config.enabled {
            UpstreamHealth::Connected
        } else {
            UpstreamHealth::Disabled
        };
        let entry = PoolEntry {
            config: config.clone(),
            snapshot,
            in_process: Some(upstream),
            live: None,
        };
        self.entries
            .write()
            .expect("upstream pool lock poisoned")
            .insert(config.name.clone(), entry);
        Ok(())
    }

    pub async fn call_tool(&self, call: ToolCall) -> Result<Value, UpstreamError> {
        self.ensure_connected(&call.upstream).await?;
        let live_peer = {
            let entries = self.entries.read().expect("upstream pool lock poisoned");
            let entry =
                entries
                    .get(&call.upstream)
                    .ok_or_else(|| UpstreamError::UnknownUpstream {
                        upstream: call.upstream.clone(),
                    })?;
            ensure_routable(entry)?;
            tools::ensure_tool_exposed(entry, &call.tool)?;
            if let Some(in_process) = &entry.in_process {
                let result = in_process.call_tool(&call)?;
                let bytes = serde_json::to_vec(&result).map_or(usize::MAX, |bytes| bytes.len());
                self.response_caps()
                    .enforce(crate::upstream::CapScope::ToolsCall, bytes)?;
                return Ok(result);
            }
            entry.live.as_ref().map(|live| live.peer())
        };
        let Some(peer) = live_peer else {
            return Err(UpstreamError::Unsupported {
                upstream: call.upstream,
                capability: "tools/call",
            });
        };
        let upstream = call.upstream.clone();
        let result = live::call_live_tool(&upstream, peer, call.tool, call.params).await?;
        let bytes = serde_json::to_vec(&result).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps()
            .enforce(crate::upstream::CapScope::ToolsCall, bytes)?;
        Ok(result)
    }

    pub async fn ensure_connected(&self, upstream: &str) -> Result<(), UpstreamError> {
        let config = {
            let entries = self.entries.read().expect("upstream pool lock poisoned");
            let entry = entries
                .get(upstream)
                .ok_or_else(|| UpstreamError::UnknownUpstream {
                    upstream: upstream.to_owned(),
                })?;
            if entry.in_process.is_some() || entry.live.is_some() || !entry.config.enabled {
                return Ok(());
            }
            entry.config.clone()
        };
        let context = live::LiveConnectContext::shared(self.response_caps());
        let (live, snapshot) = live::connect_live(&config, &SpawnGuard::default(), context).await?;
        let mut entries = self.entries.write().expect("upstream pool lock poisoned");
        let entry = entries
            .get_mut(upstream)
            .ok_or_else(|| UpstreamError::UnknownUpstream {
                upstream: upstream.to_owned(),
            })?;
        entry.snapshot = snapshot;
        entry.live = Some(Arc::new(live));
        Ok(())
    }

    pub async fn refresh_all(&self) {
        let names = self
            .entries
            .read()
            .expect("upstream pool lock poisoned")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for name in names {
            if let Err(error) = self.ensure_connected(&name).await {
                let _ = self.record_discovery_error(&name, error);
            }
        }
    }

    pub(super) fn record_discovery_error(
        &self,
        upstream: &str,
        error: UpstreamError,
    ) -> Result<(), UpstreamError> {
        let mut entries = self.entries.write().expect("upstream pool lock poisoned");
        let entry = entries
            .get_mut(upstream)
            .ok_or_else(|| UpstreamError::UnknownUpstream {
                upstream: upstream.to_owned(),
            })?;
        match error {
            UpstreamError::Unsupported {
                upstream,
                capability,
            } => {
                entry.snapshot.health = UpstreamHealth::Unsupported {
                    reason: format!("upstream `{upstream}` does not support `{capability}`"),
                };
            }
            other => {
                entry.snapshot.health = UpstreamHealth::Degraded {
                    consecutive_failures: 1,
                    error: Some(other.to_string()),
                };
                entry.snapshot.stale = true;
            }
        }
        Ok(())
    }

    fn snapshots(&self) -> Vec<UpstreamSnapshot> {
        self.entries
            .read()
            .expect("upstream pool lock poisoned")
            .values()
            .map(|entry| entry.snapshot.clone())
            .collect()
    }

    fn with_entry<T>(
        &self,
        upstream: &str,
        f: impl FnOnce(&PoolEntry) -> Result<T, UpstreamError>,
    ) -> Result<T, UpstreamError> {
        let entries = self.entries.read().expect("upstream pool lock poisoned");
        let entry = entries
            .get(upstream)
            .ok_or_else(|| UpstreamError::UnknownUpstream {
                upstream: upstream.to_owned(),
            })?;
        f(entry)
    }
}

fn ensure_routable(entry: &PoolEntry) -> Result<(), UpstreamError> {
    if entry.snapshot.health.is_routable() {
        return Ok(());
    }
    Err(UpstreamError::NotRoutable {
        upstream: entry.snapshot.name.clone(),
        reason: health_reason(&entry.snapshot.health),
    })
}

fn health_reason(health: &UpstreamHealth) -> String {
    match health {
        UpstreamHealth::Connected => "connected".to_owned(),
        UpstreamHealth::Disabled => "disabled".to_owned(),
        UpstreamHealth::Degraded { error, .. } => error
            .clone()
            .unwrap_or_else(|| "capability degraded".to_owned()),
        UpstreamHealth::Unsupported { reason } => reason.clone(),
    }
}

fn transport_for_config(config: &UpstreamConfig) -> TransportKind {
    if let Some(url) = config.url.as_deref() {
        return transport_kind_for_decision(&decide_http_transport(url));
    }
    if config.command.is_some() {
        return TransportKind::Stdio;
    }
    TransportKind::InProcess
}

fn health_for_config(name: &str, transport: TransportKind) -> UpstreamHealth {
    match transport {
        TransportKind::InProcess => UpstreamHealth::Unsupported {
            reason: format!("configured upstream `{name}` has no live in-process connector"),
        },
        TransportKind::HttpJson | TransportKind::HttpSse | TransportKind::WebSocket => {
            UpstreamHealth::Unsupported {
                reason: format!("live upstream `{name}` is not connected yet"),
            }
        }
        TransportKind::Stdio => UpstreamHealth::Unsupported {
            reason: format!("live stdio upstream `{name}` is not connected yet"),
        },
    }
}

#[cfg(test)]
#[path = "pool_tests.rs"]
mod tests;
