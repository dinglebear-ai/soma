use super::*;

impl SomaApplication {
    pub(super) async fn execute_python_environment_action(
        &self,
        request: ExecuteActionRequest,
        context: ExecutionContext,
    ) -> Result<ExecuteActionResponse, ApplicationError> {
        let limits = ProviderRequestLimits {
            max_response_bytes: context
                .response_limit
                .unwrap_or(ProviderRequestLimits::default().max_response_bytes),
            ..ProviderRequestLimits::default()
        };
        let call = ProviderCall {
            provider: String::new(),
            action: request.action.clone(),
            params: request.params.clone(),
            principal: provider_principal(context.principal.as_ref()),
            auth_mode: provider_auth_mode(context.authorization_mode),
            surface: provider_surface(context.surface),
            destructive_confirmed: context.destructive_confirmation.is_confirmed(),
            limits,
            snapshot_id: String::new(),
            request_id: context.request_id.as_str().to_owned(),
            traceparent: context
                .trace
                .as_ref()
                .and_then(|trace| trace.traceparent.clone()),
            tracestate: context
                .trace
                .as_ref()
                .and_then(|trace| trace.tracestate.clone()),
            progress: Default::default(),
        };
        let call = self.legacy_registry.authorize_operator_action(call)?;
        let action = SomaAction::from_rest(&request.action, &request.params)
            .map_err(|error| ApplicationError::service(&error))?;
        let output = match action {
            SomaAction::PythonEnvironmentStatus => self.ports.python_environment.status().await?,
            SomaAction::PythonEnvironmentPrunePlan {
                stale_before_unix_seconds,
                max_entries,
            } => {
                self.ports
                    .python_environment
                    .prune(stale_before_unix_seconds, max_entries, false)
                    .await?
            }
            SomaAction::PythonEnvironmentPrune {
                stale_before_unix_seconds,
                max_entries,
            } => {
                self.ports
                    .python_environment
                    .prune(stale_before_unix_seconds, max_entries, true)
                    .await?
            }
            SomaAction::PythonEnvironmentRepair { provider_path } => {
                let provider_path = self.managed_python_provider_path(&provider_path)?;
                self.ports.python_environment.repair(&provider_path).await?
            }
            SomaAction::PythonEnvironmentUpdate { provider_path } => {
                let provider_path = self.managed_python_provider_path(&provider_path)?;
                let update = self.ports.python_environment.update(&provider_path).await?;
                let update = update.into_report();
                let candidate = update.candidate.clone();
                let snapshot = self
                    .legacy_registry
                    .activate_python_candidate(&provider_path, candidate)
                    .await
                    .map_err(|error| {
                        ApplicationError::new(
                            error.code(),
                            crate::provider_errors::redact_public(error.message()),
                            false,
                            "Keep the active generation, inspect the candidate, and retry.",
                        )
                    })?;
                serde_json::json!({
                    "update": update,
                    "active_snapshot": {
                        "id": snapshot.id,
                        "fingerprint": snapshot.fingerprint,
                    }
                })
            }
            SomaAction::PythonWorkerStatus => self.legacy_registry.python_worker_status(),
            SomaAction::PythonWorkerCancel { provider } => {
                let cancelled = self
                    .legacy_registry
                    .cancel_python_worker(&provider)
                    .map_err(ApplicationError::from)?;
                serde_json::json!({ "provider": provider, "cancelled": cancelled })
            }
            SomaAction::PythonWorkerReset { provider } => {
                self.legacy_registry
                    .reset_python_worker_quarantine(&provider)
                    .await
                    .map_err(ApplicationError::from)?;
                serde_json::json!({ "provider": provider, "reset": true })
            }
            SomaAction::PythonGenerationStatus => self.legacy_registry.python_generation_status(),
            SomaAction::PythonGenerationRollback { generation_id } => self
                .legacy_registry
                .rollback_python_generation(generation_id)
                .await
                .map_err(ApplicationError::from)?,
            SomaAction::PythonGraduationStatus { workspace } => {
                let workspace = self.managed_graduation_path(&workspace, true)?;
                let provider_root = self.managed_provider_root()?;
                tokio::task::spawn_blocking(move || {
                    super::python_componentize::graduation_status(&workspace, &provider_root)
                })
                .await
                .map_err(|error| ApplicationError::legacy("graduation status task", error))?
                .map_err(|error| ApplicationError::legacy("graduation status", error))?
            }
            SomaAction::PythonGraduationApply {
                operation,
                workspace,
                source,
                component,
                fixtures,
                wheelhouse,
            } => {
                let workspace =
                    self.managed_graduation_path(&workspace, operation != "graduate")?;
                let source = source
                    .map(|path| self.managed_python_provider_path(&path))
                    .transpose()?;
                let component = component
                    .map(|path| self.managed_graduation_path(&path, true))
                    .transpose()?;
                let fixtures = fixtures
                    .map(|path| self.managed_graduation_path(&path, true))
                    .transpose()?;
                let wheelhouse = wheelhouse
                    .map(|path| self.managed_graduation_path(&path, true))
                    .transpose()?;
                let provider_root = self.managed_provider_root()?;
                let comparison_deadline = (operation == "compare")
                    .then(|| tokio::time::Instant::now() + std::time::Duration::from_secs(30));
                let refresh = matches!(operation.as_str(), "activate" | "rollback");
                let serialize_registry =
                    matches!(operation.as_str(), "compare" | "activate" | "rollback");
                let _refresh_lane = if operation == "compare" {
                    let deadline = comparison_deadline.expect("compare deadline initialized");
                    let remaining = deadline
                        .checked_duration_since(tokio::time::Instant::now())
                        .ok_or_else(|| {
                            ApplicationError::legacy(
                                "graduation comparison",
                                "comparison exceeded its 30 second limit",
                            )
                        })?;
                    Some(
                        tokio::time::timeout(
                            remaining,
                            self.legacy_registry.lock_refresh_lane(),
                        )
                        .await
                        .map_err(|_| {
                            ApplicationError::legacy(
                                "graduation comparison",
                                "comparison exceeded its 30 second limit waiting for refresh serialization",
                            )
                        })?,
                    )
                } else if serialize_registry {
                    Some(self.legacy_registry.lock_refresh_lane().await)
                } else {
                    None
                };
                let prior_snapshot = refresh.then(|| self.catalog_snapshot());
                let catalog = if operation == "graduate" {
                    Some(self.graduation_catalog(source.as_deref().ok_or_else(|| {
                        ApplicationError::legacy("graduation operation", "graduate requires source")
                    })?)?)
                } else {
                    None
                };
                let graduation_identity =
                    if matches!(operation.as_str(), "compare" | "activate" | "rollback") {
                        let workspace_for_identity = workspace.clone();
                        let provider_root_for_identity = provider_root.clone();
                        let identity_deadline = comparison_deadline
                            .map(tokio::time::Instant::into_std)
                            .unwrap_or_else(|| {
                                std::time::Instant::now() + std::time::Duration::from_secs(30)
                            });
                        let task = tokio::task::spawn_blocking(move || {
                            crate::graduation::identity_before(
                                &workspace_for_identity,
                                &provider_root_for_identity,
                                identity_deadline,
                            )
                        });
                        let result = if operation == "compare" {
                            let deadline =
                                comparison_deadline.expect("compare deadline initialized");
                            let remaining = deadline
                                .checked_duration_since(tokio::time::Instant::now())
                                .ok_or_else(|| {
                                    ApplicationError::legacy(
                                        "graduation comparison",
                                        "comparison exceeded its 30 second limit",
                                    )
                                })?;
                            tokio::time::timeout(remaining, task).await.map_err(|_| {
                            ApplicationError::legacy(
                                "graduation comparison",
                                "comparison exceeded its 30 second limit reading graduation state",
                            )
                        })?
                        } else {
                            task.await
                        }
                        .map_err(|error| ApplicationError::legacy("graduation task", error))?
                        .map_err(|error| ApplicationError::legacy("graduation state", error))?;
                        Some(result)
                    } else {
                        None
                    };
                if let Some(state) = &graduation_identity {
                    let source = &state.source;
                    let state_catalog = &state.catalog;
                    let snapshot = self.legacy_registry.snapshot();
                    let live_catalog = snapshot
                        .catalogs
                        .iter()
                        .find(|catalog| catalog.provider.name == state_catalog.provider.name)
                        .ok_or_else(|| {
                            ApplicationError::legacy(
                                "graduation operation",
                                "graduated provider is not active in the current registry",
                            )
                        })?;
                    let expected_source = if operation == "rollback" {
                        source.with_extension("wasm")
                    } else {
                        source.clone()
                    };
                    let live_source = live_catalog
                        .provider
                        .source
                        .as_deref()
                        .map(std::path::Path::new)
                        .and_then(|path| path.canonicalize().ok());
                    if live_source.as_deref() != expected_source.canonicalize().ok().as_deref() {
                        return Err(ApplicationError::legacy(
                            "graduation operation",
                            "graduation workspace no longer matches the active provider source",
                        ));
                    }
                    if crate::graduation::catalog_contract_digest(live_catalog).map_err(
                        |error| ApplicationError::legacy("graduation catalog digest", error),
                    )? != state.catalog_sha256
                    {
                        return Err(ApplicationError::legacy(
                            "graduation operation",
                            "graduation workspace provider contract differs from the live registry",
                        ));
                    }
                    if operation == "compare" {
                        let capabilities = &state_catalog.capabilities;
                        if super::python_graduation::side_effecting_capabilities(capabilities) {
                            return Err(ApplicationError::legacy(
                                "graduation comparison",
                                "dual-run requires a side-effect-free provider capability contract",
                            ));
                        }
                        self.legacy_registry
                            .authorize_candidate_capabilities(state_catalog, "graduation-compare")
                            .map_err(ApplicationError::from)?;
                    }
                }
                let invocation_context = call.provider_invocation().context;
                let output = if operation == "compare" {
                    let fixtures_path = fixtures.as_deref().ok_or_else(|| {
                        ApplicationError::legacy(
                            "graduation operation",
                            "compare requires fixtures",
                        )
                    })?;
                    let fixture_snapshot = crate::graduation::read_fixture_snapshot(fixtures_path)
                        .map_err(|error| ApplicationError::legacy("graduation fixtures", error))?;
                    let state_catalog = &graduation_identity
                        .as_ref()
                        .expect("compare identity prepared")
                        .catalog;
                    let snapshot = self.legacy_registry.snapshot();
                    let mut live_outputs = Vec::with_capacity(fixture_snapshot.fixtures.len());
                    for fixture in &fixture_snapshot.fixtures {
                        let input = fixture.input.as_object().ok_or_else(|| {
                            ApplicationError::legacy(
                                "graduation fixture",
                                "fixture input must be an invocation object",
                            )
                        })?;
                        if input
                            .keys()
                            .any(|key| !matches!(key.as_str(), "provider" | "action" | "arguments"))
                        {
                            return Err(ApplicationError::legacy(
                                "graduation fixture",
                                "fixture input may contain only provider, action, and arguments; execution context is host-owned",
                            ));
                        }
                        let provider =
                            input
                                .get("provider")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    ApplicationError::legacy(
                                        "graduation fixture",
                                        "fixture input requires provider",
                                    )
                                })?;
                        if provider != state_catalog.provider.name {
                            return Err(ApplicationError::legacy(
                                "graduation fixture",
                                "fixture provider does not match the live Python provider",
                            ));
                        }
                        let action =
                            input.get("action").and_then(Value::as_str).ok_or_else(|| {
                                ApplicationError::legacy(
                                    "graduation fixture",
                                    "fixture input requires action",
                                )
                            })?;
                        if snapshot.provider_for_action(action)
                            != Some(state_catalog.provider.name.as_str())
                            || !state_catalog.tools.iter().any(|tool| tool.name == action)
                        {
                            return Err(ApplicationError::legacy(
                                "graduation fixture",
                                "fixture action is not owned by the graduated Python provider",
                            ));
                        }
                        if snapshot.action_requires_confirmation(action)
                            || state_catalog
                                .tools
                                .iter()
                                .any(|tool| tool.name == action && tool.destructive)
                        {
                            return Err(ApplicationError::legacy(
                                "graduation fixture",
                                "destructive actions cannot be replayed during graduation comparison",
                            ));
                        }
                        let arguments = input.get("arguments").cloned().ok_or_else(|| {
                            ApplicationError::legacy(
                                "graduation fixture",
                                "fixture input requires arguments",
                            )
                        })?;
                        let mut live_call = call.clone();
                        live_call.provider = provider.to_owned();
                        live_call.action = action.to_owned();
                        live_call.params = arguments;
                        live_call.destructive_confirmed = false;
                        live_call.snapshot_id = snapshot.id.clone();
                        let effective_input = serde_json::to_value(live_call.execution_envelope())
                            .map_err(|error| {
                                ApplicationError::legacy(
                                    "graduation fixture",
                                    format!("could not serialize effective invocation: {error}"),
                                )
                            })?;
                        let remaining = comparison_deadline
                            .and_then(|deadline| {
                                deadline.checked_duration_since(tokio::time::Instant::now())
                            })
                            .ok_or_else(|| {
                                ApplicationError::legacy(
                                    "graduation comparison",
                                    "comparison exceeded its 30 second limit",
                                )
                            })?;
                        let live = tokio::time::timeout(
                            remaining,
                            self.legacy_registry.dispatch(live_call),
                        )
                        .await
                        .map_err(|_| {
                            ApplicationError::legacy(
                                "graduation comparison",
                                "live Python comparison exceeded its 30 second limit",
                            )
                        })?
                        .map_err(ApplicationError::from)?;
                        live_outputs.push((effective_input, live.value));
                    }
                    crate::graduation::compare(
                        &workspace,
                        crate::graduation::ComparisonRequest {
                            component: component.as_deref(),
                            fixtures: fixture_snapshot,
                            live_runs: live_outputs,
                            context: &invocation_context,
                            provider_root: &provider_root,
                            deadline: comparison_deadline.expect("compare deadline initialized"),
                            max_response_bytes: call.limits.max_response_bytes,
                        },
                    )
                    .await
                    .map_err(|error| ApplicationError::legacy("graduation operation", error))?
                } else {
                    let operation_task = super::python_componentize::GraduationOperation {
                        operation: operation.clone(),
                        workspace: workspace.clone(),
                        source,
                        component,
                        fixtures,
                        wheelhouse,
                        catalog,
                        provider_root: provider_root.clone(),
                    };
                    tokio::task::spawn_blocking(move || operation_task.run())
                        .await
                        .map_err(|error| ApplicationError::legacy("graduation task", error))?
                        .map_err(|error| ApplicationError::legacy("graduation operation", error))?
                };
                if refresh {
                    let refreshed = self
                        .legacy_registry
                        .refresh_file_providers_strict_in_lane()
                        .await;
                    let expected_source = if operation == "activate" {
                        output.get("deployed_component")
                    } else {
                        output
                            .get("source")
                            .or_else(|| output.get("deployed_component"))
                    }
                    .and_then(serde_json::Value::as_str);
                    let expected_version = output
                        .get("active")
                        .and_then(|active| active.get("sha256"))
                        .and_then(serde_json::Value::as_str)
                        .map(|digest| format!("sha256:{digest}"));
                    let verified = refreshed.as_ref().is_ok_and(|snapshot| {
                        expected_source.is_some_and(|expected| {
                            snapshot.catalogs.iter().any(|catalog| {
                                catalog.provider.source.as_deref() == Some(expected)
                                    && expected_version.as_deref().is_none_or(|version| {
                                        catalog.provider.version.as_deref() == Some(version)
                                    })
                            })
                        })
                    });
                    if !verified {
                        let refresh_error = refreshed.err().map_or_else(
                            || {
                                "refreshed generation did not contain the promoted provider"
                                    .to_owned()
                            },
                            |error| error.to_string(),
                        );
                        crate::graduation::recover(&workspace, &provider_root).map_err(
                            |recovery| {
                                ApplicationError::legacy(
                                    "graduation recovery",
                                    format!("{refresh_error}; recovery failed: {recovery}"),
                                )
                            },
                        )?;
                        let restored = self
                            .legacy_registry
                            .refresh_file_providers_strict_in_lane()
                            .await
                            .map_err(|error| {
                                ApplicationError::legacy("graduation recovery refresh", error)
                            })?;
                        if prior_snapshot
                            .as_ref()
                            .is_none_or(|prior| restored.fingerprint != prior.fingerprint)
                        {
                            return Err(ApplicationError::legacy(
                                "graduation recovery",
                                "provider registry did not restore the prior generation",
                            ));
                        }
                        return Err(ApplicationError::legacy(
                            "graduation activation",
                            refresh_error,
                        ));
                    }
                    if let Err(commit_error) = crate::graduation::commit_transaction(&workspace) {
                        if crate::graduation::is_ambiguous_commit(&commit_error) {
                            return Err(ApplicationError::new(
                                "graduation_commit_ambiguous",
                                commit_error.to_string(),
                                false,
                                "Do not retry or roll back automatically. Inspect the active provider and retained transaction tombstone, then run startup recovery cleanup.",
                            ));
                        }
                        crate::graduation::recover(&workspace, &provider_root).map_err(
                            |recovery| {
                                ApplicationError::legacy(
                                    "graduation commit recovery",
                                    format!("{commit_error}; recovery failed: {recovery}"),
                                )
                            },
                        )?;
                        let restored = self
                            .legacy_registry
                            .refresh_file_providers_strict_in_lane()
                            .await
                            .map_err(|error| {
                                ApplicationError::legacy(
                                    "graduation commit recovery refresh",
                                    error,
                                )
                            })?;
                        if prior_snapshot
                            .as_ref()
                            .is_none_or(|prior| restored.fingerprint != prior.fingerprint)
                        {
                            return Err(ApplicationError::legacy(
                                "graduation commit recovery",
                                "provider registry did not restore the prior generation",
                            ));
                        }
                        return Err(ApplicationError::legacy("graduation commit", commit_error));
                    }
                }
                output
            }
            _ => {
                return Err(ApplicationError::new(
                    "invalid_python_environment_action",
                    "action is not a Python environment operation",
                    false,
                    "Use one of the documented Python environment actions.",
                ));
            }
        };
        self.enforce_python_response_limit(output, &call, &context)
    }

    fn graduation_catalog(
        &self,
        source: &std::path::Path,
    ) -> Result<soma_provider_core::ProviderCatalog, ApplicationError> {
        let canonical = source
            .canonicalize()
            .map_err(|error| ApplicationError::legacy("canonicalize Python provider", error))?;
        self.legacy_registry
            .snapshot()
            .catalogs
            .iter()
            .find(|catalog| {
                matches!(
                    catalog.provider.kind,
                    soma_provider_core::ProviderKind::Python
                        | soma_provider_core::ProviderKind::Langchain
                        | soma_provider_core::ProviderKind::Llamaindex
                ) && catalog.provider.source.as_deref().is_some_and(|candidate| {
                    std::path::Path::new(candidate)
                        .canonicalize()
                        .is_ok_and(|candidate| candidate == canonical)
                })
            })
            .cloned()
            .ok_or_else(|| {
                ApplicationError::new(
                    "graduation_provider_not_active",
                    "graduation source is not an active Python provider",
                    false,
                    "Refresh the provider registry and use an active managed Python source.",
                )
            })
    }

    fn managed_provider_root(&self) -> Result<std::path::PathBuf, ApplicationError> {
        self.legacy_registry
            .file_provider_root()
            .ok_or_else(|| {
                ApplicationError::legacy(
                    "graduation provider root",
                    "dynamic file providers are not configured",
                )
            })?
            .canonicalize()
            .map_err(|error| ApplicationError::legacy("canonicalize provider root", error))
    }

    fn managed_python_provider_path(
        &self,
        provider_path: &str,
    ) -> Result<std::path::PathBuf, ApplicationError> {
        self.legacy_registry
            .resolve_python_provider_path(std::path::Path::new(provider_path))
            .map_err(|error| {
                ApplicationError::new(
                    error.code(),
                    error.message(),
                    false,
                    "Use a managed Python provider path and retry.",
                )
            })
    }

    fn managed_graduation_path(
        &self,
        raw: &str,
        must_exist: bool,
    ) -> Result<std::path::PathBuf, ApplicationError> {
        let root = std::env::var_os("SOMA_GRADUATION_ROOT")
            .map(std::path::PathBuf::from)
            .ok_or_else(|| {
                ApplicationError::new(
                    "graduation_root_unconfigured",
                    "SOMA_GRADUATION_ROOT is required for agent-facing graduation actions",
                    false,
                    "Configure an absolute, operator-owned graduation root.",
                )
            })?;
        let root = root
            .canonicalize()
            .map_err(|error| ApplicationError::legacy("canonicalize graduation root", error))?;
        let requested = std::path::PathBuf::from(raw);
        if !requested.is_absolute() {
            return Err(ApplicationError::new(
                "graduation_path_invalid",
                "graduation paths must be absolute",
                false,
                "Use a path inside SOMA_GRADUATION_ROOT.",
            ));
        }
        let resolved = if must_exist {
            requested
                .canonicalize()
                .map_err(|error| ApplicationError::legacy("canonicalize graduation path", error))?
        } else {
            let parent = requested.parent().ok_or_else(|| {
                ApplicationError::new(
                    "graduation_path_invalid",
                    "graduation path has no parent",
                    false,
                    "Use a path inside SOMA_GRADUATION_ROOT.",
                )
            })?;
            parent
                .canonicalize()
                .map_err(|error| ApplicationError::legacy("canonicalize graduation parent", error))?
                .join(requested.file_name().ok_or_else(|| {
                    ApplicationError::new(
                        "graduation_path_invalid",
                        "graduation path has no file name",
                        false,
                        "Use a path inside SOMA_GRADUATION_ROOT.",
                    )
                })?)
        };
        if !resolved.starts_with(&root) {
            return Err(ApplicationError::new(
                "graduation_path_outside_root",
                "graduation path is outside SOMA_GRADUATION_ROOT",
                false,
                "Use an operator-managed graduation path.",
            ));
        }
        Ok(resolved)
    }

    fn enforce_python_response_limit(
        &self,
        output: Value,
        call: &ProviderCall,
        context: &ExecutionContext,
    ) -> Result<ExecuteActionResponse, ApplicationError> {
        let response = ExecuteActionResponse {
            output,
            request_id: context.request_id.as_str().to_owned(),
            progress: Vec::new(),
        };
        response.enforce_serialized_limit(call.limits.max_response_bytes)?;
        Ok(response)
    }
}
