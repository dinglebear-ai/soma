use super::*;

#[test]
fn final_operation_set_is_closed() {
    for name in [
        "docker.rmi",
        "docker.prune",
        "compose.down",
        "files.transfer",
    ] {
        assert!(final_operation(&OperationName::new(name).unwrap()));
    }
    assert!(!final_operation(
        &OperationName::new("docker.pull").unwrap()
    ));
}
