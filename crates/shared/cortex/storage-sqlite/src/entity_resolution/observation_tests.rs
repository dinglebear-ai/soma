use super::*;

#[test]
fn display_values_are_bounded_and_sensitive_values_are_redacted() {
    assert_eq!(
        safe_display_value("https://user:pass@example.test"),
        "[redacted]"
    );
    assert_eq!(safe_display_value("/home/alice/token.txt"), "[redacted]");
    let long = "x".repeat(200);
    assert_eq!(safe_display_value(&long).len(), 128);
    assert_eq!(
        safe_display_value(
            "a
b	c"
        ),
        "abc"
    );
}

#[test]
fn resolver_trust_orders_strongest_first() {
    assert!(ResolverTrust::Verified < ResolverTrust::Claimed);
    assert!(ResolverTrust::Claimed < ResolverTrust::Inferred);
}
