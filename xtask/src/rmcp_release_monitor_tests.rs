
#[test]
fn exact_version_strips_cargo_requirement_comparators() {
    // The workspace pins rmcp exactly; the monitor must still parse it.
    assert_eq!(super::exact_version("=3.0.0-beta.2"), "3.0.0-beta.2");
    assert_eq!(super::exact_version("=2.2.0"), "2.2.0");
    assert_eq!(super::exact_version("^1.2.3"), "1.2.3");
    assert_eq!(super::exact_version("~1.2.3"), "1.2.3");
    assert_eq!(super::exact_version(" 1.2.3 "), "1.2.3");
    assert!(semver::Version::parse(super::exact_version("=3.0.0-beta.2")).is_ok());
}
