use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use serde_json::Value;

use crate::config::UpstreamConfig;
use crate::upstream::http_client::{decide_http_transport, transport_kind_for_decision};
use crate::upstream::{
    ResponseCaps, ToolDescriptor, TransportKind, UpstreamError, UpstreamHealth, UpstreamSnapshot,
};

pub mod connect_stdio;
pub mod discovery;
pub mod health;
pub mod prompts;
pub mod resources;
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

#[derive(Debug, Clone)]
struct PoolEntry {
    config: UpstreamConfig,
    snapshot: UpstreamSnapshot,
    in_process: Option<InProcessUpstream>,
}

#[derive(Debug, Clone)]
pub struct UpstreamPool {
    entries: Arc<RwLock<BTreeMap<String, PoolEntry>>>,
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
            health_for_transport(config.name.as_str(), transport)
        } else {
            UpstreamHealth::Disabled
        };
        let mut snapshot = UpstreamSnapshot::empty(config.name.clone(), transport);
        snapshot.health = health;
        let entry = PoolEntry {
            config: config.clone(),
            snapshot,
            in_process: None,
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
        };
        self.entries
            .write()
            .expect("upstream pool lock poisoned")
            .insert(config.name.clone(), entry);
        Ok(())
    }

    pub fn call_tool(&self, call: ToolCall) -> Result<Value, UpstreamError> {
        let entries = self.entries.read().expect("upstream pool lock poisoned");
        let entry = entries
            .get(&call.upstream)
            .ok_or_else(|| UpstreamError::UnknownUpstream {
                upstream: call.upstream.clone(),
            })?;
        ensure_routable(entry)?;
        tools::ensure_tool_exposed(entry, &call.tool)?;
        let Some(in_process) = &entry.in_process else {
            return Err(UpstreamError::Unsupported {
                upstream: call.upstream,
                capability: "tools/call",
            });
        };
        let result = in_process.call_tool(&call)?;
        let bytes = serde_json::to_vec(&result).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps()
            .enforce(crate::upstream::CapScope::ToolsCall, bytes)?;
        Ok(result)
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

fn health_for_transport(name: &str, transport: TransportKind) -> UpstreamHealth {
    if matches!(transport, TransportKind::WebSocketUnsupported) {
        return UpstreamHealth::Unsupported {
            reason: format!("websocket upstream `{name}` is not supported by soma-gateway yet"),
        };
    }
    UpstreamHealth::Connected
}

#[cfg(test)]
#[path = "pool_tests.rs"]
mod tests;
