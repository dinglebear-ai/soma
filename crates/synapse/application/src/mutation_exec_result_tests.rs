use super::*;

#[test]
fn evidence_uris_preserve_execution_identity() {
    assert_eq!(
        exec_evidence_uri("host-exec", "dookie", "rg"),
        "host-exec://dookie/rg"
    );
}
