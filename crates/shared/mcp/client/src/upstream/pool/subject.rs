use serde_json::{Map, Value};

use crate::oauth::UpstreamOAuthProvider;
use crate::process::guard::SpawnGuard;
use crate::upstream::{
    CapScope, PromptDescriptor, ResourceDescriptor, ToolDescriptor, UpstreamError, UpstreamHealth,
    UpstreamSnapshot,
};

use super::tools::matches_filter;
use super::{live, SubjectPoolEntry, ToolCall, UpstreamPool};

impl UpstreamPool {
    pub fn install_oauth_provider(&self, provider: std::sync::Arc<dyn UpstreamOAuthProvider>) {
        *self
            .oauth_provider
            .write()
            .expect("oauth provider lock poisoned") = Some(provider);
        self.subject_entries
            .write()
            .expect("subject pool lock poisoned")
            .clear();
    }

    pub fn evict_oauth_subject(&self, upstream: &str, subject: &str) {
        self.subject_entries
            .write()
            .expect("subject pool lock poisoned")
            .remove(&(upstream.to_owned(), subject.to_owned()));
        if let Some(provider) = self
            .oauth_provider
            .read()
            .expect("oauth provider lock poisoned")
            .as_ref()
        {
            provider.evict_subject(upstream, subject);
        }
    }

    pub async fn discover_for_subject(
        &self,
        subject: Option<&str>,
    ) -> Result<Vec<UpstreamSnapshot>, UpstreamError> {
        let Some(subject) = subject else {
            return self.discover().await;
        };
        let configs = self.configured_upstreams();
        for (name, oauth_enabled) in configs {
            let result = if oauth_enabled {
                self.ensure_subject_connected(&name, subject).await
            } else {
                self.ensure_connected(&name).await
            };
            if let Err(error) = result {
                tracing::warn!(upstream = %name, subject, error = %error, "subject discovery failed");
            }
        }
        Ok(self.snapshots_for_subject(subject))
    }

    pub async fn exposed_tools_for_subject(
        &self,
        upstream: &str,
        subject: Option<&str>,
    ) -> Result<Vec<ToolDescriptor>, UpstreamError> {
        let Some(subject) = subject else {
            return self.exposed_tools(upstream);
        };
        let Some((snapshot, config)) = self.subject_snapshot_and_config(upstream, subject)? else {
            return self.exposed_tools(upstream);
        };
        let tools: Vec<ToolDescriptor> = snapshot
            .tools
            .into_iter()
            .filter(|tool| matches_filter(config.expose_tools.as_deref(), &tool.name))
            .collect();
        let bytes = serde_json::to_vec(&tools).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps().enforce(CapScope::ToolsList, bytes)?;
        Ok(tools)
    }

    pub async fn call_tool_for_subject(
        &self,
        call: ToolCall,
        subject: Option<&str>,
    ) -> Result<Value, UpstreamError> {
        let Some(subject) = subject else {
            return self.call_tool(call).await;
        };
        if !self.config_is_oauth(&call.upstream)? {
            return self.call_tool(call).await;
        }
        self.ensure_subject_connected(&call.upstream, subject)
            .await?;
        let (peer, upstream) = self.with_subject_entry(&call.upstream, subject, |entry| {
            ensure_subject_routable(&entry.snapshot)?;
            if !entry
                .snapshot
                .tools
                .iter()
                .any(|candidate| candidate.name == call.tool)
            {
                return Err(UpstreamError::NotExposed {
                    upstream: call.upstream.clone(),
                    item: call.tool.clone(),
                });
            }
            Ok((entry.live.peer(), entry.snapshot.name.clone()))
        })?;
        let result = live::call_live_tool(&upstream, peer, call.tool, call.params).await?;
        let bytes = serde_json::to_vec(&result).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps().enforce(CapScope::ToolsCall, bytes)?;
        Ok(result)
    }

    pub async fn list_resources_for_subject(
        &self,
        upstream: &str,
        subject: Option<&str>,
    ) -> Result<Vec<ResourceDescriptor>, UpstreamError> {
        let Some(subject) = subject else {
            return self.list_resources(upstream).await;
        };
        let Some((snapshot, config)) = self.subject_snapshot_and_config(upstream, subject)? else {
            return self.list_resources(upstream).await;
        };
        if !config.proxy_resources {
            return Ok(Vec::new());
        }
        let resources: Vec<ResourceDescriptor> = snapshot
            .resources
            .into_iter()
            .filter(|resource| matches_filter(config.expose_resources.as_deref(), &resource.uri))
            .collect();
        let bytes = serde_json::to_vec(&resources).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps()
            .enforce(CapScope::ResourcesList, bytes)?;
        Ok(resources)
    }

    pub async fn read_resource_for_subject(
        &self,
        upstream: &str,
        uri: &str,
        subject: Option<&str>,
    ) -> Result<Value, UpstreamError> {
        let Some(subject) = subject else {
            return self.read_resource(upstream, uri).await;
        };
        if !self.config_is_oauth(upstream)? {
            return self.read_resource(upstream, uri).await;
        }
        self.ensure_subject_connected(upstream, subject).await?;
        let peer = self.with_subject_entry(upstream, subject, |entry| {
            ensure_subject_routable(&entry.snapshot)?;
            Ok(entry.live.peer())
        })?;
        let value = live::read_live_resource(upstream, peer, uri.to_owned()).await?;
        let bytes = serde_json::to_vec(&value).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps()
            .enforce(CapScope::ResourcesRead, bytes)?;
        Ok(value)
    }

    pub async fn list_prompts_for_subject(
        &self,
        upstream: &str,
        subject: Option<&str>,
    ) -> Result<Vec<PromptDescriptor>, UpstreamError> {
        let Some(subject) = subject else {
            return self.list_prompts(upstream).await;
        };
        let Some((snapshot, config)) = self.subject_snapshot_and_config(upstream, subject)? else {
            return self.list_prompts(upstream).await;
        };
        if !config.proxy_prompts {
            return Ok(Vec::new());
        }
        let prompts: Vec<PromptDescriptor> = snapshot
            .prompts
            .into_iter()
            .filter(|prompt| matches_filter(config.expose_prompts.as_deref(), &prompt.name))
            .collect();
        let bytes = serde_json::to_vec(&prompts).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps().enforce(CapScope::PromptsList, bytes)?;
        Ok(prompts)
    }

    pub async fn get_prompt_for_subject(
        &self,
        upstream: &str,
        name: &str,
        arguments: Option<Map<String, Value>>,
        subject: Option<&str>,
    ) -> Result<Value, UpstreamError> {
        let Some(subject) = subject else {
            return self.get_prompt(upstream, name, arguments).await;
        };
        if !self.config_is_oauth(upstream)? {
            return self.get_prompt(upstream, name, arguments).await;
        }
        self.ensure_subject_connected(upstream, subject).await?;
        let peer = self.with_subject_entry(upstream, subject, |entry| {
            ensure_subject_routable(&entry.snapshot)?;
            Ok(entry.live.peer())
        })?;
        let value = live::get_live_prompt(upstream, peer, name.to_owned(), arguments).await?;
        let bytes = serde_json::to_vec(&value).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps().enforce(CapScope::PromptsGet, bytes)?;
        Ok(value)
    }

    async fn ensure_subject_connected(
        &self,
        upstream: &str,
        subject: &str,
    ) -> Result<(), UpstreamError> {
        let key = (upstream.to_owned(), subject.to_owned());
        if self
            .subject_entries
            .read()
            .expect("subject pool lock poisoned")
            .contains_key(&key)
        {
            return Ok(());
        }
        let config = self.config_for_subject(upstream)?;
        if !config.enabled {
            return Ok(());
        }
        if config.oauth.is_none() {
            return self.ensure_connected(upstream).await;
        }
        let provider = self.oauth_provider()?;
        let context = live::LiveConnectContext::oauth(self.response_caps(), subject, provider);
        let (live, snapshot) = live::connect_live(&config, &SpawnGuard::default(), context).await?;
        self.subject_entries
            .write()
            .expect("subject pool lock poisoned")
            .insert(
                key,
                SubjectPoolEntry {
                    snapshot,
                    live: std::sync::Arc::new(live),
                },
            );
        Ok(())
    }

    fn configured_upstreams(&self) -> Vec<(String, bool)> {
        self.entries
            .read()
            .expect("upstream pool lock poisoned")
            .iter()
            .map(|(name, entry)| (name.clone(), entry.config.oauth.is_some()))
            .collect()
    }

    fn snapshots_for_subject(&self, subject: &str) -> Vec<UpstreamSnapshot> {
        let entries = self.entries.read().expect("upstream pool lock poisoned");
        let subject_entries = self
            .subject_entries
            .read()
            .expect("subject pool lock poisoned");
        entries
            .iter()
            .filter_map(|(name, entry)| {
                if entry.config.oauth.is_some() {
                    return subject_entries
                        .get(&(name.clone(), subject.to_owned()))
                        .map(|entry| entry.snapshot.clone());
                }
                Some(entry.snapshot.clone())
            })
            .collect()
    }

    fn subject_snapshot_and_config(
        &self,
        upstream: &str,
        subject: &str,
    ) -> Result<Option<(UpstreamSnapshot, crate::config::UpstreamConfig)>, UpstreamError> {
        if !self.config_is_oauth(upstream)? {
            return Ok(None);
        }
        let snapshot =
            self.with_subject_entry(upstream, subject, |entry| Ok(entry.snapshot.clone()))?;
        let config = self.config_for_subject(upstream)?;
        Ok(Some((snapshot, config)))
    }

    fn with_subject_entry<T>(
        &self,
        upstream: &str,
        subject: &str,
        f: impl FnOnce(&SubjectPoolEntry) -> Result<T, UpstreamError>,
    ) -> Result<T, UpstreamError> {
        let entries = self
            .subject_entries
            .read()
            .expect("subject pool lock poisoned");
        let entry = entries
            .get(&(upstream.to_owned(), subject.to_owned()))
            .ok_or_else(|| UpstreamError::NotRoutable {
                upstream: upstream.to_owned(),
                reason: "subject connection is not established".to_owned(),
            })?;
        f(entry)
    }

    fn oauth_provider(&self) -> Result<std::sync::Arc<dyn UpstreamOAuthProvider>, UpstreamError> {
        self.oauth_provider
            .read()
            .expect("oauth provider lock poisoned")
            .clone()
            .ok_or_else(|| UpstreamError::LiveConnect {
                upstream: "oauth".to_owned(),
                message: "upstream OAuth runtime is not configured".to_owned(),
            })
    }

    fn config_for_subject(
        &self,
        upstream: &str,
    ) -> Result<crate::config::UpstreamConfig, UpstreamError> {
        self.with_entry(upstream, |entry| Ok(entry.config.clone()))
    }

    fn config_is_oauth(&self, upstream: &str) -> Result<bool, UpstreamError> {
        self.with_entry(upstream, |entry| Ok(entry.config.oauth.is_some()))
    }
}

fn ensure_subject_routable(snapshot: &UpstreamSnapshot) -> Result<(), UpstreamError> {
    if snapshot.health == UpstreamHealth::Connected {
        return Ok(());
    }
    Err(UpstreamError::NotRoutable {
        upstream: snapshot.name.clone(),
        reason: "subject-scoped upstream is not connected".to_owned(),
    })
}

#[cfg(test)]
#[path = "subject_tests.rs"]
mod tests;
