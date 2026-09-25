use super::*;

#[test]
fn evidence_uris_preserve_execution_identity() {
    assert_eq!(
        exec_evidence_uri("host-exec", "devhost", "rg"),
        "host-exec://devhost/rg"
    );
}
