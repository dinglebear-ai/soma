use super::*;
use crate::SynapseConfig;

#[test]
fn openapi_contains_every_execute_and_mutation_plan_path() {
    let runtime = StandaloneRuntime::from_config(SynapseConfig::default()).unwrap();
    let document = document(&runtime);
    let paths = document["paths"].as_object().unwrap();
    let execute = paths
        .keys()
        .filter(|path| path.ends_with("/execute"))
        .count();
    let plans = paths.keys().filter(|path| path.ends_with("/plan")).count();
    assert_eq!(execute, 59);
    assert_eq!(plans, 21);
}
