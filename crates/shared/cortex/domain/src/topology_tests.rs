use super::*;

#[test]
fn topology_finding_wire_shape_omits_empty_details() {
    let entity = TopologyFindingEntity {
        entity_type: "host".into(),
        key: "dookie".into(),
        label: "DOOKIE".into(),
        details: Default::default(),
    };
    let value = serde_json::to_value(entity).unwrap();
    assert!(value.get("details").is_none());
    assert!(topology_findings::TYPES.contains(&topology_findings::TYPE_COLLECTOR_HEALTH));
}
