//! Soma's product-domain invariants (plan section 6.2): the action catalog,
//! error taxonomy, scope/token-budget policy, and CLI provider-validation
//! rules shared identically across REST, CLI, and MCP dispatch.
//!
//! Split out of `soma-contracts` in PR 13 rather than `soma-application`
//! because `soma-service` (a dependency of `soma-application` during the
//! PR 12 strangler migration) also builds its static-Rust provider catalog
//! directly from these types; every consumer (application, service, api,
//! cli, mcp, integrations, runtime, apps/soma) can already depend on
//! `soma-domain` without creating a cycle. See the per-module
//! "module-placement rationale" notes (e.g. the bottom of `actions.rs`) for
//! the full reasoning.

mod execution;
mod principal;

pub mod actions;
pub mod errors;
pub mod provider_validation;
pub mod scopes;
pub mod token_limit;

pub use execution::{
    AuthorizationMode, Confirmation, RequestId, RequestIdError, Surface, TraceContext,
};
pub use principal::{Principal, ScopeSet};
