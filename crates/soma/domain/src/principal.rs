use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// A deduplicated, order-stable set of scope strings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScopeSet(BTreeSet<String>);

impl ScopeSet {
    /// Builds a set from the given scopes, dropping empty strings.
    pub fn new(scopes: impl IntoIterator<Item = String>) -> Self {
        Self(
            scopes
                .into_iter()
                .filter(|scope| !scope.is_empty())
                .collect(),
        )
    }

    /// Returns true if `scope` is present in the set.
    pub fn contains(&self, scope: &str) -> bool {
        self.0.contains(scope)
    }

    /// Returns the scopes as a sorted `Vec`.
    pub fn to_vec(&self) -> Vec<String> {
        self.0.iter().cloned().collect()
    }
}

impl<const N: usize> From<[&str; N]> for ScopeSet {
    fn from(scopes: [&str; N]) -> Self {
        Self::new(scopes.into_iter().map(ToOwned::to_owned))
    }
}

/// An authenticated caller identity and the scopes it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    /// Identifier of the caller (e.g. token subject).
    pub subject: String,
    /// Scopes granted to this caller.
    pub scopes: ScopeSet,
    /// Token issuer, when known.
    pub issuer: Option<String>,
}

impl Principal {
    /// Builds a principal with the given subject and scopes and no issuer.
    pub fn new(subject: impl Into<String>, scopes: ScopeSet) -> Self {
        Self {
            subject: subject.into(),
            scopes,
            issuer: None,
        }
    }

    /// Returns a copy of this principal with the issuer set.
    pub fn with_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    /// An unauthenticated principal with no scopes.
    pub fn anonymous() -> Self {
        Self::new("anonymous", ScopeSet::default())
    }
}

#[cfg(test)]
mod tests {
    use super::ScopeSet;

    #[test]
    fn scope_sets_are_deduplicated_and_stable() {
        let scopes = ScopeSet::new([
            "soma:write".to_owned(),
            "soma:read".to_owned(),
            "soma:write".to_owned(),
        ]);
        assert_eq!(
            scopes.to_vec(),
            vec!["soma:read".to_owned(), "soma:write".to_owned()]
        );
    }
}
