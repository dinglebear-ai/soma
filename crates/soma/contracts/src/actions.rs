//! Deprecated compatibility facade. `SomaAction`, `ACTION_SPECS`, and every
//! other symbol here now live in `soma-domain` (plan section 6.2 "From
//! soma-contracts"; see `soma_domain::actions` for the real implementation
//! and the rationale for landing in `soma-domain` rather than
//! `soma-application`). This module re-exports them for one migration
//! window; new code should import `soma_domain::actions` directly. PR 19
//! deletes this crate.

pub use soma_domain::actions::*;

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
