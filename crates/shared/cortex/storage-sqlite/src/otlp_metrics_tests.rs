use super::*;

#[test]
fn instrument_kind_wire_values_round_trip() {
    for kind in MetricInstrumentKind::ALL {
        let json = serde_json::to_string(kind).unwrap();
        let decoded: MetricInstrumentKind = serde_json::from_str(&json).unwrap();
        assert_eq!(*kind, decoded);
        assert_eq!(kind.to_string(), kind.as_str());
    }
    assert!("bogus".parse::<MetricInstrumentKind>().is_err());
}
