//! Product launcher execution and auth policy for
//! `POST /v1/palette/execute`.
//!
//! Dispatch itself (`SomaApplication::execute_action`) does not know about
//! Palette surface exposure — that policy is `ToolSpec.palette` overlay data,
//! which only the surface layers interpret. This module is the Palette
//! surface's enforcement point: an id that isn't (or is no longer)
//! palette-exposed is rejected here as `launcher_not_found`, before it ever
//! reaches the application layer.

use soma_application::{ApplicationError, ExecuteActionRequest, ExecutionContext};
use soma_domain::Confirmation;

use crate::{
    dto::{LauncherExecuteRequest, LauncherExecuteResponse},
    schema::find_schema,
    state::PaletteState,
};

pub enum ExecuteOutcome {
    Ok(LauncherExecuteResponse),
    NotFound,
    Failed(ApplicationError),
}

/// Resolve and dispatch a Palette launcher execute request. Thin by design:
/// verify the id is still a palette-exposed action, set destructive
/// confirmation from the request, and delegate to
/// `SomaApplication::execute_action` for everything else (scope, admin,
/// destructive, and capability enforcement all live there).
pub async fn execute_launcher(
    state: &PaletteState,
    request: LauncherExecuteRequest,
    mut context: ExecutionContext,
) -> ExecuteOutcome {
    let snapshot = match state.application().refresh_providers() {
        Ok(snapshot) => snapshot,
        Err(error) => return ExecuteOutcome::Failed(error),
    };
    if find_schema(&snapshot, &request.id).is_none() {
        return ExecuteOutcome::NotFound;
    }

    context.destructive_confirmation = confirmation_for(request.confirm_destructive);

    let action_request = ExecuteActionRequest {
        action: request.id,
        params: request.params,
    };

    match state
        .application()
        .execute_action(action_request, context)
        .await
    {
        Ok(response) => ExecuteOutcome::Ok(LauncherExecuteResponse {
            output: response.output,
            request_id: response.request_id,
        }),
        Err(error) => ExecuteOutcome::Failed(error),
    }
}

/// Pure translation of the request's `confirmDestructive` flag into domain
/// `Confirmation`. Split out from [`execute_launcher`] (which additionally
/// needs a live `SomaApplication` to resolve/dispatch) so the policy is
/// unit-testable on its own.
#[must_use]
fn confirmation_for(confirm_destructive: bool) -> Confirmation {
    if confirm_destructive {
        Confirmation::Confirmed
    } else {
        Confirmation::Missing
    }
}

#[cfg(test)]
#[path = "execute_tests.rs"]
mod tests;
