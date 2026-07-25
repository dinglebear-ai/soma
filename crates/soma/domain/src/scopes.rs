//! Canonical Soma authorization scope constants.
//!
//! Single source of truth for every scope name Soma issues or checks, and for
//! the write-implies-read satisfaction rule. Formerly split across
//! `soma-contracts`' `actions.rs` (`READ_SCOPE`/`WRITE_SCOPE`/`DENY_SCOPE`/
//! `scopes_satisfy`) and `scopes.rs` (`ADMIN_SCOPE`/`has_admin_scope`);
//! merged here since they are the same invariant-value concept.

/// Read scope: grants access to read-only actions.
pub const READ_SCOPE: &str = "soma:read";
/// Write scope: grants mutating actions and satisfies [`READ_SCOPE`].
pub const WRITE_SCOPE: &str = "soma:write";
/// Sentinel scope no token can hold; assigned to unknown actions to deny them.
pub const DENY_SCOPE: &str = "soma:__deny__";
/// Admin scope: grants administrative actions.
pub const ADMIN_SCOPE: &str = "soma:admin";

/// Returns true if `token_scopes` satisfy `required`.
/// Write scope satisfies read (write includes read).
/// Single source of truth - called from both REST and MCP enforcement paths.
pub fn scopes_satisfy(token_scopes: &[String], required: &str) -> bool {
    token_scopes
        .iter()
        .any(|s| s == required || (required == READ_SCOPE && s == WRITE_SCOPE))
}

/// Returns true if `scopes` contains [`ADMIN_SCOPE`].
pub fn has_admin_scope(scopes: &[String]) -> bool {
    scopes.iter().any(|scope| scope == ADMIN_SCOPE)
}

#[cfg(test)]
#[path = "scopes_tests.rs"]
mod tests;
