use super::*;

pub(super) fn subscription_resource_uris(
    resources: &[crate::upstream::ResourceDescriptor],
    expose_resources: Option<&[String]>,
) -> Vec<String> {
    resources
        .iter()
        .filter(|resource| super::super::tools::matches_filter(expose_resources, &resource.uri))
        .map(|resource| resource.uri.clone())
        .collect()
}

pub(super) fn requested_subscription_filter(
    capabilities: &rmcp::model::ServerCapabilities,
    proxy_resources: bool,
    proxy_prompts: bool,
    resource_uris: Vec<String>,
) -> SubscriptionFilter {
    let mut builder = SubscriptionFilter::builder().tools_list_changed();
    if proxy_prompts {
        builder = builder.prompts_list_changed();
    }
    if proxy_resources {
        builder = builder
            .resources_list_changed()
            .resource_subscriptions(resource_uris);
    }
    builder.build().supported_by(capabilities)
}

impl UpstreamPool {
    /// Start or replace one long-lived `subscriptions/listen` stream for a
    /// shared upstream connection. Legacy protocol versions are deliberately
    /// skipped because `subscriptions/listen` is a 2026-07-28 contract.
    pub(in crate::upstream::pool) async fn refresh_upstream_subscription(&self, upstream: &str) {
        let generation = self.begin_subscription_generation(upstream);
        let mut setup_guard = SubscriptionGenerationGuard::new(
            &self.subscription_tasks,
            upstream,
            Arc::clone(&generation),
        );
        let connection = {
            let entries = self.entries.read().expect("upstream pool lock poisoned");
            entries.get(upstream).and_then(|entry| {
                entry.live.as_ref().map(|live| {
                    (
                        live.peer(),
                        subscription_resource_uris(
                            &entry.snapshot.resources,
                            entry.config.expose_resources.as_deref(),
                        ),
                        entry.config.proxy_resources,
                        entry.config.proxy_prompts,
                    )
                })
            })
        };
        let Some((peer, resource_uris, proxy_resources, proxy_prompts)) = connection else {
            self.retire_subscription_generation_if_current(upstream, &generation);
            return;
        };
        let Some(server_info) = peer.peer_info() else {
            self.retire_subscription_generation_if_current(upstream, &generation);
            return;
        };
        if !subscription_listen_supported_protocol(&server_info.protocol_version) {
            self.retire_subscription_generation_if_current(upstream, &generation);
            return;
        }
        let requested = requested_subscription_filter(
            &server_info.capabilities,
            proxy_resources,
            proxy_prompts,
            resource_uris,
        );
        if subscription_filter_is_empty(&requested) {
            self.retire_subscription_generation_if_current(upstream, &generation);
            return;
        }

        let initial = tokio::select! {
            biased;
            () = generation.cancelled() => {
                self.retire_subscription_generation_if_current(upstream, &generation);
                return;
            }
            result = tokio::time::timeout(
                SUBSCRIPTION_ESTABLISH_TIMEOUT,
                peer.listen(requested.clone()),
            ) => result,
        };
        let initial_subscription = match initial {
            Ok(Ok(subscription)) => Some(subscription),
            Ok(Err(error)) if terminal_subscription_listen_error(&error, 0) => {
                tracing::debug!(upstream, error = %error, "upstream subscriptions/listen unsupported");
                self.retire_subscription_generation_if_current(upstream, &generation);
                return;
            }
            Ok(Err(error)) => {
                tracing::warn!(upstream, error = %error, "initial subscriptions/listen failed; scheduling retry");
                None
            }
            Err(_) => {
                tracing::warn!(
                    upstream,
                    timeout_ms = SUBSCRIPTION_ESTABLISH_TIMEOUT.as_millis(),
                    "initial subscriptions/listen timed out; scheduling retry"
                );
                None
            }
        };

        let subscription_tasks = Arc::downgrade(&self.subscription_tasks);
        let notification_tx = self.notification_tx.clone();
        let upstream = upstream.to_owned();
        tokio::spawn(async move {
            let mut initial_subscription = initial_subscription;
            let mut retry_attempt = 0_u32;
            if initial_subscription.is_none() {
                let delay = subscription_retry_delay(&upstream, retry_attempt);
                retry_attempt = retry_attempt.saturating_add(1);
                tokio::select! {
                    biased;
                    () = generation.cancelled() => {
                        retire_generation_if_current_weak(
                            &subscription_tasks,
                            &upstream,
                            &generation,
                        );
                        return;
                    }
                    () = tokio::time::sleep(delay) => {}
                }
            }

            loop {
                let mut subscription = match initial_subscription.take() {
                    Some(subscription) => subscription,
                    None => {
                        let established = tokio::select! {
                            biased;
                            () = generation.cancelled() => break,
                            result = tokio::time::timeout(
                                SUBSCRIPTION_ESTABLISH_TIMEOUT,
                                peer.listen(requested.clone()),
                            ) => result,
                        };
                        match established {
                            Ok(Ok(subscription)) => subscription,
                            Ok(Err(error))
                                if terminal_subscription_listen_error(&error, retry_attempt) =>
                            {
                                tracing::warn!(upstream = %upstream, error = %error, "subscriptions/listen retry suppressed");
                                break;
                            }
                            Ok(Err(error)) => {
                                tracing::warn!(upstream = %upstream, error = %error, retry_attempt, "subscriptions/listen retry failed");
                                let delay = subscription_retry_delay(&upstream, retry_attempt);
                                retry_attempt = retry_attempt.saturating_add(1);
                                tokio::select! {
                                    biased;
                                    () = generation.cancelled() => break,
                                    () = tokio::time::sleep(delay) => continue,
                                }
                            }
                            Err(_) => {
                                tracing::warn!(upstream = %upstream, retry_attempt, "subscriptions/listen retry timed out");
                                let delay = subscription_retry_delay(&upstream, retry_attempt);
                                retry_attempt = retry_attempt.saturating_add(1);
                                tokio::select! {
                                    biased;
                                    () = generation.cancelled() => break,
                                    () = tokio::time::sleep(delay) => continue,
                                }
                            }
                        }
                    }
                };

                let established_at = tokio::time::Instant::now();
                loop {
                    let next = tokio::select! {
                        biased;
                        () = generation.cancelled() => {
                            drop(subscription);
                            retire_generation_if_current_weak(
                                &subscription_tasks,
                                &upstream,
                                &generation,
                            );
                            return;
                        }
                        result = subscription.next() => result,
                    };
                    let event = match next {
                        Ok(Some(ServerNotification::ToolListChangedNotification(_))) => {
                            Some(UpstreamNotificationEvent::ToolListChanged {
                                upstream: upstream.clone(),
                            })
                        }
                        Ok(Some(ServerNotification::PromptListChangedNotification(_))) => {
                            Some(UpstreamNotificationEvent::PromptListChanged {
                                upstream: upstream.clone(),
                            })
                        }
                        Ok(Some(ServerNotification::ResourceListChangedNotification(_))) => {
                            Some(UpstreamNotificationEvent::ResourceListChanged {
                                upstream: upstream.clone(),
                            })
                        }
                        Ok(Some(ServerNotification::ResourceUpdatedNotification(notification))) => {
                            resource_updated_event(&upstream, notification.params.uri)
                        }
                        Ok(Some(_)) => None,
                        Ok(None) => break,
                        Err(error) => {
                            tracing::warn!(upstream = %upstream, error = %error, "upstream subscription stream ended with error");
                            break;
                        }
                    };
                    if let Some(event) = event
                        && !publish_event_if_current(
                            &subscription_tasks,
                            &notification_tx,
                            &upstream,
                            &generation,
                            event,
                        )
                    {
                        drop(subscription);
                        return;
                    }
                }

                retry_attempt =
                    next_subscription_retry_attempt(retry_attempt, established_at.elapsed());
                let delay = subscription_retry_delay(&upstream, retry_attempt);
                tokio::select! {
                    biased;
                    () = generation.cancelled() => break,
                    () = tokio::time::sleep(delay) => {}
                }
            }
            retire_generation_if_current_weak(&subscription_tasks, &upstream, &generation);
        });
        setup_guard.disarm();
    }
}
