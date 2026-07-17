//! Smoke test for the deprecated facade: confirms `soma_contracts::errors`
//! still resolves to the real `soma_domain::errors` implementation. Full
//! behavioral coverage lives in `soma-domain`.

use super::*;

#[test]
fn facade_reexports_the_real_error_type() {
    let error = ToolError::validation("bad_field", "Bad field", "Use a better value.");
    assert_eq!(error.kind, ServiceErrorKind::Validation);
    assert_eq!(error.http_status_code(), 400);
}
