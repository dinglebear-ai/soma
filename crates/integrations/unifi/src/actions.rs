pub mod hybrid;
pub mod internal;
pub mod official;

use serde_json::Value;

use crate::capabilities::find_capability;
use crate::error::{Result, UnifiError};
use crate::{api::ApiSourceFamily, UnifiClient};

/// A dynamically-dispatched UniFi action: an action name matched against
/// [`crate::capabilities::all_capabilities`], plus its JSON parameters.
#[derive(Debug, Clone)]
pub struct ActionRequest {
    /// Action name, e.g. `"clients"` or `"official_list_devices"`.
    pub action: String,
    /// Action parameters. Shape depends on the action; see its
    /// [`crate::capabilities::Capability`] for the expected `path`/`query`/`body`.
    pub params: Value,
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
