//! The catalog of every action this crate can dispatch, assembled once from
//! the two `data/*.json` inventories baked into the crate at compile time.

use std::sync::OnceLock;

use crate::api::ApiSourceFamily;

pub mod internal_network;
pub mod official_network;

/// Who is allowed to call a [`Capability`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthScope {
    /// Read-only; safe for any authenticated caller.
    Read,
    /// Mutates controller state; callers should gate this behind an
    /// explicit admin/write permission.
    Admin,
}

impl AuthScope {
    /// The scope's wire/config representation (`"read"` or `"admin"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Admin => "admin",
        }
    }
}

/// One dispatchable UniFi action: its name, which API serves it, and (for
/// non-hybrid actions) the method/path template to call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    /// Action name, as passed to [`crate::ActionRequest::action`].
    pub action: String,
    /// Human-readable summary, e.g. for listing available actions in a UI.
    pub title: String,
    /// Which API this capability is served from.
    pub source: ApiSourceFamily,
    /// HTTP method. `None` only for [`ApiSourceFamily::Hybrid`] entries,
    /// which resolve to another capability's method at dispatch time.
    pub method: Option<String>,
    /// Path template, e.g. `/v1/sites/{siteId}`. `None` only for
    /// [`ApiSourceFamily::Hybrid`] entries.
    pub path: Option<String>,
    /// Whether this action changes controller state.
    pub mutating: bool,
    /// Minimum caller permission required.
    pub auth_scope: AuthScope,
    /// Provenance/confidence tag from the source inventory (e.g.
    /// `"contract_ok"`, `"legacy_alias"`); informational only.
    pub verification_mode: Option<String>,
}

/// Every capability this crate can dispatch, built once and cached for the
/// process lifetime.
///
/// # Panics
/// Panics if either bundled `data/*.json` inventory fails to parse. Both
/// files ship with the crate and are covered by this crate's own tests, so
/// a panic here means the crate itself was built or edited incorrectly —
/// never a condition a caller can hit at runtime with valid input.
pub fn all_capabilities() -> &'static [Capability] {
    static ALL: OnceLock<Vec<Capability>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut caps = Vec::new();
        caps.extend(official_network::capabilities());
        caps.extend(internal_network::capabilities());
        caps
    })
}

/// Looks up a capability by [`Capability::action`] name.
pub fn find_capability(action: &str) -> Option<&'static Capability> {
    all_capabilities().iter().find(|cap| cap.action == action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_capabilities_has_no_duplicate_action_names() {
        let mut names: Vec<&str> = all_capabilities()
            .iter()
            .map(|cap| cap.action.as_str())
            .collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();

        assert_eq!(names.len(), before, "duplicate capability action name");
    }

    #[test]
    fn find_capability_finds_a_known_action() {
        assert!(find_capability("clients").is_some());
    }

    #[test]
    fn find_capability_returns_none_for_an_unknown_action() {
        assert!(find_capability("does_not_exist").is_none());
    }
}
