#[test]
fn package_exposes_canonical_runtime() {
    assert_eq!(
        synapse_application::SynapseCatalog::embedded().operation_count(),
        59
    );
}
