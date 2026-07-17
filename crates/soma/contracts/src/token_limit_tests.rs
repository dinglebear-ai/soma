//! Smoke test for the deprecated facade: confirms
//! `soma_contracts::token_limit` still resolves to the real
//! `soma_domain::token_limit` implementation. Full behavioral coverage
//! lives in `soma-domain`.

use super::*;

#[test]
fn facade_reexports_the_real_response_cap() {
    assert_eq!(MAX_RESPONSE_BYTES, 40_000);
    assert_eq!(truncate_if_needed("hello"), "hello");
}
