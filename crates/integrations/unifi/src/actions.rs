pub mod hybrid;
pub mod internal;
pub mod official;

use serde_json::Value;

use crate::capabilities::find_capability;
use crate::error::{Result, UnifiError};
use crate::{UnifiClient, api::ApiSourceFamily};

/// A dynamically-dispatched UniFi action: an action name matched against
/// [`crate::capabilities::all_capabilities`], plus its JSON parameters.
///
/// `#[non_exhaustive]`: unlike this crate's other public types, this one
/// *is* meant to be constructed by callers — use [`ActionRequest::new`]
/// rather than a struct literal, so a future added field (e.g. a per-call
/// timeout override) can default instead of being a downstream semver
/// break.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ActionRequest {
    /// Action name, e.g. `"clients"` or `"official_list_devices"`.
    pub action: String,
    /// Action parameters. Shape depends on the action; see its
    /// [`crate::capabilities::Capability`] for the expected `path`/`query`/`body`.
    pub params: Value,
}

impl ActionRequest {
    /// Builds a request for `action` with `params`.
    pub fn new(action: impl Into<String>, params: Value) -> Self {
        Self {
            action: action.into(),
            params,
        }
    }
}

/// Looks up an [`ActionRequest`]'s action against the capability catalog and
/// runs it against the official, internal, or hybrid API as appropriate.
///
/// Holds one [`UnifiClient`] (and therefore one pooled `reqwest::Client`) for
/// its lifetime — construct once per controller and reuse it across calls
/// rather than rebuilding per [`execute`](Self::execute) call.
pub struct ActionDispatcher {
    client: UnifiClient,
}

impl ActionDispatcher {
    /// Wraps an already-built [`UnifiClient`].
    pub fn new(client: UnifiClient) -> Self {
        Self { client }
    }

    /// Runs `request` against whichever API family its action belongs to.
    ///
    /// # Errors
    /// Returns [`UnifiError::UnknownAction`] if `request.action` has no
    /// registered capability; see [`UnifiError`] for the other failure cases
    /// this can return.
    pub async fn execute(&self, request: ActionRequest) -> Result<Value> {
        let Some(capability) = find_capability(&request.action) else {
            return Err(UnifiError::UnknownAction(request.action));
        };
        match capability.source {
            ApiSourceFamily::Official => {
                official::execute(&self.client, capability, &request.params).await
            }
            ApiSourceFamily::Internal => {
                internal::execute(&self.client, capability, &request.params).await
            }
            ApiSourceFamily::Hybrid => {
                let (target, params) =
                    hybrid::resolve(capability.action.as_str(), &request.params)?;
                // Intentionally InvalidRequest, not UnknownAction: `request.action`
                // itself was a registered hybrid capability. This branch only
                // fires if hybrid::resolve's own routing table names a target
                // action that isn't in the catalog — a bug in this crate's data,
                // not a caller-supplied bad action name.
                let Some(target_capability) = find_capability(target) else {
                    return Err(UnifiError::InvalidRequest {
                        context: capability.action.clone(),
                        message: format!("hybrid action resolved to unknown action {target}"),
                    });
                };
                match target_capability.source {
                    ApiSourceFamily::Official => {
                        official::execute(&self.client, target_capability, &params).await
                    }
                    ApiSourceFamily::Internal => {
                        internal::execute(&self.client, target_capability, &params).await
                    }
                    ApiSourceFamily::Hybrid => Err(UnifiError::HybridRouting(format!(
                        "{} resolved to another hybrid action",
                        capability.action
                    ))),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_accepts_a_string_or_a_str_and_sets_both_fields() {
        let owned = ActionRequest::new("clients".to_string(), json!({ "a": 1 }));
        let borrowed = ActionRequest::new("clients", json!({ "a": 1 }));

        assert_eq!(owned.action, "clients");
        assert_eq!(owned.params, json!({ "a": 1 }));
        assert_eq!(borrowed.action, "clients");
        assert_eq!(borrowed.params, json!({ "a": 1 }));
    }
}
