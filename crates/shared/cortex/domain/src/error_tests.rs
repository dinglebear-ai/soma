use super::*;

#[test]
fn domain_error_preserves_caller_facing_messages() {
    assert_eq!(
        DomainError::InvalidInput("bad timestamp".into()).to_string(),
        "bad timestamp"
    );
    assert_eq!(
        DomainError::NotFound("missing host".into()).to_string(),
        "missing host"
    );
}
