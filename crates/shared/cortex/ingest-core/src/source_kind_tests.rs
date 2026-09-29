use super::*;

#[test]
fn wire_values_match_donor_contract() {
    assert_eq!(
        SourceKind::all_wire_names(),
        vec![
            "syslog-udp",
            "syslog-tcp",
            "docker-stream",
            "docker-event",
            "otlp",
            "adguard-api",
            "unifi-api",
            "agent",
            "shell-history",
            "agent-command",
            "file-tail",
        ]
    );
}

#[test]
fn wire_round_trip_and_syslog_classification_are_stable() {
    for kind in SourceKind::ALL {
        assert_eq!(SourceKind::from_wire(kind.as_str()), Some(kind));
    }
    assert_eq!(
        SourceKind::from_wire(" syslog-udp "),
        Some(SourceKind::SyslogUdp)
    );
    assert_eq!(SourceKind::from_wire("syslog_udp"), None);
    assert!(SourceKind::SyslogUdp.is_syslog());
    assert!(SourceKind::SyslogTcp.is_syslog());
    assert!(!SourceKind::DockerStream.is_syslog());
    assert_eq!(AGENT_DOCKER_SOURCE_KIND, "agent-docker");
}
