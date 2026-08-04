use super::*;

#[test]
fn canonical_run_requires_an_operation_and_json_object() {
    let cli = Cli::try_parse_from(["synapse", "run", "product.help"]).unwrap();
    assert!(matches!(cli.command, Command::Run(_)));
    assert!(input_value("[]", None).is_err());
}

#[test]
fn legacy_tool_names_are_closed() {
    assert!(Cli::try_parse_from(["synapse", "legacy", "flux"]).is_ok());
    assert!(Cli::try_parse_from(["synapse", "legacy", "other"]).is_err());
}
