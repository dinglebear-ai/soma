use super::DockerClientProvider;

#[test]
fn provider_contract_is_object_safe() {
    fn accepts_provider(_: Option<&dyn DockerClientProvider>) {}
    accepts_provider(None);
}
