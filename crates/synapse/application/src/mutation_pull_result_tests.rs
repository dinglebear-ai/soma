use soma_ops::VerificationStatus;

use super::*;

#[test]
fn inconclusive_pull_verification_uses_stable_diagnostic_code() {
    let diagnostic = verification_diagnostic(
        VerificationStatus::Inconclusive,
        "image store could not be read".into(),
        "inspect Docker",
    )
    .unwrap();
    assert_eq!(diagnostic.code(), "verification.inconclusive");
}
