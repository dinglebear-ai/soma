use std::time::Duration;

use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    service::{RequestContext, SubscriptionContext},
};

use super::SomaRmcpServer;

const CATALOG_POLL_INTERVAL: Duration = Duration::from_millis(750);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct CatalogSnapshot {
    tools: Vec<String>,
    resources: Vec<String>,
    prompts: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct CatalogChanges {
    pub(super) tools: bool,
    pub(super) resources: bool,
    pub(super) prompts: bool,
}

impl CatalogSnapshot {
    pub(super) fn changes_from(&self, previous: &Self) -> CatalogChanges {
        CatalogChanges {
            tools: self.tools != previous.tools,
            resources: self.resources != previous.resources,
            prompts: self.prompts != previous.prompts,
        }
    }
}

pub(super) async fn listen_for_catalog_changes(
    server: &SomaRmcpServer,
    context: SubscriptionContext,
) -> Result<(), ErrorData> {
    let request = context.request_context().clone();
    let accepted = context.accepted().clone();
    let sink = context.sink().clone();
    let mut previous = catalog_snapshot(server, &request).await?;
    let mut interval = tokio::time::interval(CATALOG_POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Consume the immediate first tick so the first comparison occurs after one
    // full coalescing window rather than re-reading the same contract instantly.
    interval.tick().await;

    loop {
        tokio::select! {
            () = context.cancelled() => break,
            _ = interval.tick() => {
                let next = match catalog_snapshot(server, &request).await {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "MCP catalog subscription snapshot failed; retaining the last published contract"
                        );
                        continue;
                    }
                };
                let changes = next.changes_from(&previous);
                if !changes.tools && !changes.resources && !changes.prompts {
                    continue;
                }

                let send_result = async {
                    if changes.tools && accepted.tools_list_changed == Some(true) {
                        sink.notify_tool_list_changed().await?;
                    }
                    if changes.resources && accepted.resources_list_changed == Some(true) {
                        sink.notify_resource_list_changed().await?;
                    }
                    if changes.prompts && accepted.prompts_list_changed == Some(true) {
                        sink.notify_prompt_list_changed().await?;
                    }
                    Ok::<(), rmcp::service::SubscriptionSendError>(())
                }
                .await;
                if let Err(error) = send_result {
                    tracing::debug!(
                        error = %error,
                        "MCP catalog subscription peer closed or rejected notification"
                    );
                    break;
                }
                previous = next;
            }
        }
    }

    Ok(())
}

async fn catalog_snapshot(
    server: &SomaRmcpServer,
    request: &RequestContext<RoleServer>,
) -> Result<CatalogSnapshot, ErrorData> {
    let mut tools = server
        .list_tools(None, request.clone())
        .await?
        .tools
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect::<Vec<_>>();
    tools.sort_unstable();
    tools.dedup();

    let resource_result = server.list_resources(None, request.clone()).await?;
    let template_result = server
        .list_resource_templates(None, request.clone())
        .await?;
    let mut resources = resource_result
        .resources
        .into_iter()
        .map(|resource| format!("resource:{}", resource.uri))
        .chain(
            template_result
                .resource_templates
                .into_iter()
                .map(|template| format!("template:{}", template.uri_template)),
        )
        .collect::<Vec<_>>();
    resources.sort_unstable();
    resources.dedup();

    let mut prompts = server
        .list_prompts(None, request.clone())
        .await?
        .prompts
        .into_iter()
        .map(|prompt| prompt.name)
        .collect::<Vec<_>>();
    prompts.sort_unstable();
    prompts.dedup();

    Ok(CatalogSnapshot {
        tools,
        resources,
        prompts,
    })
}

#[cfg(test)]
#[path = "catalog_subscriptions_tests.rs"]
mod tests;
