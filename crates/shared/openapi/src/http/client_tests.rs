#[test]
fn dispatch_client_builds_with_rustls_provider() {
    let client = super::build_dispatch_client().expect("dispatch client");
    drop(client);
}

#[test]
fn loopback_test_client_builds_for_unit_tests() {
    let client = super::client::build_loopback_test_client();
    drop(client);
}
