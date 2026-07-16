use super::caller::CodeModeCaller;

#[test]
fn trusted_local_sets_admin_capabilities() {
    let caller = CodeModeCaller::trusted_local("cli");
    assert!(caller.capabilities.trusted_local);
    assert!(caller.capabilities.admin);
}
