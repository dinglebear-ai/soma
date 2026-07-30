use super::*;

const GENERATION_HISTORY_LIMIT: usize = 3;

#[derive(Clone)]
pub(super) struct StoredGeneration {
    pub(super) generation_id: u64,
    pub(super) providers: BTreeMap<String, Arc<dyn Provider>>,
    pub(super) core_registry: CoreRegistry,
    pub(super) snapshot: Arc<RegistrySnapshot>,
    pub(super) file_fingerprint: Option<String>,
    pub(super) python_environments: PythonProviderEnvironmentSelections,
}

pub(super) struct GenerationTransition {
    suspended: Vec<Arc<dyn Provider>>,
    evicted: Vec<Arc<dyn Provider>>,
}

impl ProviderRegistry {
    /// Returns the active generation and bounded rollback history.
    pub fn python_generation_status(&self) -> Value {
        let state = self
            .state
            .read()
            .expect("provider registry lock should not be poisoned");
        let describe = |generation_id: u64,
                        snapshot: &Arc<RegistrySnapshot>,
                        file_fingerprint: &Option<String>,
                        environments: &PythonProviderEnvironmentSelections| {
            json!({
                "generation_id": generation_id,
                "snapshot_id": snapshot.id,
                "catalog_fingerprint": snapshot.fingerprint,
                "file_fingerprint": file_fingerprint,
                "python_environment_count": environments.len(),
            })
        };
        json!({
            "active": describe(
                state.generation_id,
                &state.snapshot,
                &state.file_fingerprint,
                &state.python_environments,
            ),
            "rollback_candidates": state.history.iter().map(|generation| {
                describe(
                    generation.generation_id,
                    &generation.snapshot,
                    &generation.file_fingerprint,
                    &generation.python_environments,
                )
            }).collect::<Vec<_>>(),
        })
    }

    /// Atomically reactivates a retained immutable provider generation.
    pub async fn rollback_python_generation(
        &self,
        target_generation_id: u64,
    ) -> Result<Value, ProviderError> {
        let target_environments = {
            let state = self
                .state
                .read()
                .expect("provider registry lock should not be poisoned");
            state
                .history
                .iter()
                .find(|generation| generation.generation_id == target_generation_id)
                .map(|generation| generation.python_environments.clone())
                .ok_or_else(|| unavailable_generation(target_generation_id))?
        };
        let rollback_fingerprint = if let Some(file_source) = self.file_source.clone() {
            let environments = target_environments.clone();
            Some(
                tokio::task::spawn_blocking(move || {
                    file_source.fingerprint_with_python_environments(&environments)
                })
                .await
                .map_err(fingerprint_error)?
                .map_err(fingerprint_error)?,
            )
        } else {
            None
        };
        let (report, transition) = {
            let mut state = self
                .state
                .write()
                .expect("provider registry lock should not be poisoned");
            let position = state
                .history
                .iter()
                .position(|generation| generation.generation_id == target_generation_id)
                .ok_or_else(|| unavailable_generation(target_generation_id))?;
            let target = state
                .history
                .remove(position)
                .expect("located generation must remain present");
            let previous_generation_id = state.generation_id;
            let transition = publish_generation(
                &mut state,
                target.providers,
                target.core_registry,
                target.snapshot,
                rollback_fingerprint,
                target.python_environments,
            );
            (
                json!({
                    "previous_generation_id": previous_generation_id,
                    "restored_generation_id": target_generation_id,
                    "active_generation_id": state.generation_id,
                    "snapshot_id": state.snapshot.id,
                }),
                transition,
            )
        };
        settle_transition_async(transition).await;
        Ok(report)
    }
}

fn unavailable_generation(target_generation_id: u64) -> ProviderError {
    ProviderError::validation(
        "registry",
        "python_generation_rollback",
        "python_generation_unavailable",
        format!("generation `{target_generation_id}` is not in the rollback window"),
    )
}

fn fingerprint_error(error: impl std::fmt::Display) -> ProviderError {
    ProviderError::new(
        "provider_file_fingerprint_failed",
        "registry",
        Some("python_generation_rollback".to_owned()),
        error.to_string(),
        "Inspect the provider directory and retry.",
    )
}

pub(super) fn publish_generation(
    state: &mut RegistryState,
    providers: BTreeMap<String, Arc<dyn Provider>>,
    core_registry: CoreRegistry,
    snapshot: Arc<RegistrySnapshot>,
    file_fingerprint: Option<String>,
    python_environments: PythonProviderEnvironmentSelections,
) -> GenerationTransition {
    let suspended: Vec<_> = state
        .providers
        .values()
        .filter(|provider| {
            !providers
                .values()
                .any(|candidate| Arc::ptr_eq(candidate, provider))
        })
        .cloned()
        .collect();
    for provider in &suspended {
        provider.deactivate();
    }
    for provider in providers.values() {
        provider.activate();
    }
    state.history.push_front(StoredGeneration {
        generation_id: state.generation_id,
        providers: state.providers.clone(),
        core_registry: state.core_registry.clone(),
        snapshot: state.snapshot.clone(),
        file_fingerprint: state.file_fingerprint.clone(),
        python_environments: state.python_environments.clone(),
    });
    state.generation_id = state.generation_id.saturating_add(1);
    state.providers = providers;
    state.core_registry = core_registry;
    state.snapshot = snapshot;
    state.file_fingerprint = file_fingerprint;
    state.python_environments = python_environments;

    let mut evicted = Vec::new();
    while state.history.len() > GENERATION_HISTORY_LIMIT {
        if let Some(generation) = state.history.pop_back() {
            for provider in generation.providers.into_values() {
                let retained = state
                    .providers
                    .values()
                    .chain(
                        state
                            .history
                            .iter()
                            .flat_map(|generation| generation.providers.values()),
                    )
                    .any(|candidate| Arc::ptr_eq(candidate, &provider));
                if !retained {
                    evicted.push(provider);
                }
            }
        }
    }
    GenerationTransition { suspended, evicted }
}

pub(super) fn settle_transition(transition: GenerationTransition) {
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        runtime.spawn(settle_transition_async(transition));
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("provider transition runtime should build")
        .block_on(settle_transition_async(transition));
}

pub(super) async fn settle_transition_async(transition: GenerationTransition) {
    for provider in transition.suspended {
        provider.suspend().await;
    }
    for provider in transition.evicted {
        provider.retire().await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct LifecycleProbe {
        suspended: AtomicUsize,
        retired: AtomicUsize,
    }

    #[async_trait]
    impl Provider for LifecycleProbe {
        fn catalog(&self) -> ProviderCatalog {
            panic!("lifecycle settlement does not inspect catalogs")
        }

        async fn call(&self, _call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
            panic!("lifecycle settlement does not dispatch")
        }

        async fn suspend(&self) {
            self.suspended.fetch_add(1, Ordering::SeqCst);
        }

        async fn retire(&self) {
            self.retired.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn synchronous_transition_settles_without_an_ambient_tokio_runtime() {
        let probe = Arc::new(LifecycleProbe {
            suspended: AtomicUsize::new(0),
            retired: AtomicUsize::new(0),
        });
        settle_transition(GenerationTransition {
            suspended: vec![probe.clone()],
            evicted: vec![probe.clone()],
        });
        assert_eq!(probe.suspended.load(Ordering::SeqCst), 1);
        assert_eq!(probe.retired.load(Ordering::SeqCst), 1);
    }
}
