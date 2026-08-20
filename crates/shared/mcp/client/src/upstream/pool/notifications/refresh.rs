use crate::upstream::CapScope;

use super::super::{catalog_pagination, live};
use super::*;

impl UpstreamPool {
    fn live_for_catalog_refresh(&self, upstream: &str) -> Option<Arc<live::LiveUpstream>> {
        let entries = self.entries.read().expect("upstream pool lock poisoned");
        entries.get(upstream).and_then(|entry| entry.live.clone())
    }

    fn commit_catalog_refresh_if_current(
        &self,
        upstream: &str,
        live: &Arc<live::LiveUpstream>,
        apply: impl FnOnce(&mut crate::upstream::UpstreamSnapshot),
    ) -> bool {
        let mut entries = self.entries.write().expect("upstream pool lock poisoned");
        let Some(entry) = entries.get_mut(upstream) else {
            return false;
        };
        if !entry
            .live
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, live))
        {
            tracing::debug!(
                upstream,
                "discarding catalog refresh from replaced upstream peer"
            );
            return false;
        }
        apply(&mut entry.snapshot);
        entry.snapshot.stale = false;
        true
    }

    /// Re-list one exact upstream after it reports `tools/list_changed` and
    /// replace the cached tool snapshot only if the same live peer still owns
    /// the entry.
    pub async fn refresh_tools_after_list_changed(&self, upstream: &str) -> bool {
        let Some(live) = self.live_for_catalog_refresh(upstream) else {
            tracing::warn!(
                upstream,
                "cannot refresh tools after list_changed: live peer missing"
            );
            return false;
        };
        let peer = live.peer();
        let tools = match catalog_pagination::list_tools(
            &peer,
            self.response_caps().limit_for(CapScope::ToolsList),
        )
        .await
        {
            Ok(tools) => tools
                .into_iter()
                .map(live::tool_descriptor_for_pool)
                .collect(),
            Err(error) => {
                tracing::warn!(upstream, error = %error, "tool refresh after list_changed failed");
                return false;
            }
        };
        self.commit_catalog_refresh_if_current(upstream, &live, |snapshot| {
            snapshot.tools = tools;
        })
    }

    /// Re-list one exact upstream after `resources/list_changed` without
    /// allowing an older peer to overwrite a replacement connection. If the
    /// visible URI set changes, replace the listen generation so resource
    /// updates for newly exposed resources are actually subscribed.
    pub async fn refresh_resources_after_list_changed(&self, upstream: &str) -> bool {
        let (live, expose_resources) = {
            let entries = self.entries.read().expect("upstream pool lock poisoned");
            let Some(entry) = entries.get(upstream) else {
                return false;
            };
            if !entry.config.proxy_resources {
                return false;
            }
            let Some(live) = entry.live.clone() else {
                tracing::warn!(
                    upstream,
                    "cannot refresh resources after list_changed: live peer missing"
                );
                return false;
            };
            (live, entry.config.expose_resources.clone())
        };
        let peer = live.peer();
        let resources = match catalog_pagination::list_resources(
            &peer,
            self.response_caps().limit_for(CapScope::ResourcesList),
        )
        .await
        {
            Ok(resources) => resources
                .into_iter()
                .map(live::resource_descriptor_for_pool)
                .collect::<Vec<_>>(),
            Err(error) => {
                tracing::warn!(upstream, error = %error, "resource refresh after list_changed failed");
                return false;
            }
        };
        let visible_uris = |resources: &[crate::upstream::ResourceDescriptor]| {
            resources
                .iter()
                .filter(|resource| {
                    super::super::tools::matches_filter(expose_resources.as_deref(), &resource.uri)
                })
                .map(|resource| resource.uri.clone())
                .collect::<std::collections::BTreeSet<_>>()
        };
        let new_visible_uris = visible_uris(&resources);
        let visible_uris_changed = {
            let mut entries = self.entries.write().expect("upstream pool lock poisoned");
            let Some(entry) = entries.get_mut(upstream) else {
                return false;
            };
            if !entry
                .live
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &live))
            {
                tracing::debug!(
                    upstream,
                    "discarding resource refresh from replaced upstream peer"
                );
                return false;
            }
            let changed = visible_uris(&entry.snapshot.resources) != new_visible_uris;
            entry.snapshot.resources = resources;
            entry.snapshot.stale = false;
            changed
        };
        if visible_uris_changed {
            self.refresh_upstream_subscription(upstream).await;
        }
        true
    }

    /// Re-list one exact upstream after `prompts/list_changed` without
    /// allowing an older peer to overwrite a replacement connection.
    pub async fn refresh_prompts_after_list_changed(&self, upstream: &str) -> bool {
        let live = {
            let entries = self.entries.read().expect("upstream pool lock poisoned");
            let Some(entry) = entries.get(upstream) else {
                return false;
            };
            if !entry.config.proxy_prompts {
                return false;
            }
            let Some(live) = entry.live.clone() else {
                tracing::warn!(
                    upstream,
                    "cannot refresh prompts after list_changed: live peer missing"
                );
                return false;
            };
            live
        };
        let peer = live.peer();
        let prompts = match catalog_pagination::list_prompts(
            &peer,
            self.response_caps().limit_for(CapScope::PromptsList),
        )
        .await
        {
            Ok(prompts) => prompts
                .into_iter()
                .map(live::prompt_descriptor_for_pool)
                .collect(),
            Err(error) => {
                tracing::warn!(upstream, error = %error, "prompt refresh after list_changed failed");
                return false;
            }
        };
        self.commit_catalog_refresh_if_current(upstream, &live, |snapshot| {
            snapshot.prompts = prompts;
        })
    }
}
