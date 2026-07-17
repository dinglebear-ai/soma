//! Smoke test for the deprecated facade: confirms
//! `soma_contracts::env_registry` still resolves to the real
//! `soma_config::env_registry` implementation. Full behavioral coverage
//! lives in `soma-config`.

use super::*;

#[test]
fn facade_reexports_the_real_env_registry() {
    assert!(spec_for("SOMA_API_URL").is_some());
}
