use super::*;

#[test]
fn default_config_is_local_and_confined_to_current_directory() {
    let config = SynapseConfig::default();
    config.validate().unwrap();
    assert_eq!(config.hosts.len(), 1);
    assert!(matches!(config.hosts[0].endpoint, EndpointConfig::Local));
    assert!(config.hosts[0].read_roots[0].is_absolute());
}

#[test]
fn config_rejects_duplicate_hosts_and_relative_roots() {
    let mut config = SynapseConfig::default();
    config.hosts.push(config.hosts[0].clone());
    assert!(config.validate().is_err());

    let mut config = SynapseConfig::default();
    config.hosts[0].read_roots = vec![PathBuf::from("relative")];
    assert!(config.validate().is_err());
}
