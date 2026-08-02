use super::*;

#[test]
fn parses_service_network_mount_port_and_usage_shapes() {
    let services = parse_services(
        "sshd.service loaded active running OpenSSH daemon
",
    );
    assert_eq!(services[0].unit, "sshd.service");
    assert_eq!(services[0].description, "OpenSSH daemon");

    let network = parse_network(r#"[{"ifindex":2,"ifname":"eth0","operstate":"UP","mtu":1500,"addr_info":[{"family":"inet","local":"10.0.0.2","prefixlen":24}]}]"#).unwrap();
    assert_eq!(network[0].addresses[0].address, "10.0.0.2");

    let mounts = parse_mounts(r#"{"filesystems":[{"target":"/","source":"/dev/sda1","fstype":"ext4","size":100,"used":40,"avail":60,"children":[{"target":"/boot","source":"/dev/sda2","fstype":"ext4"}]}]}"#).unwrap();
    assert_eq!(mounts.len(), 2);

    let ports = parse_ports(
        r#"tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:(("sshd"))
"#,
    );
    assert_eq!(ports[0].local_address, "0.0.0.0:22");

    let usage = parse_usage(
        "Filesystem Type 1B-blocks Used Available Use% Mounted on
/dev/sda1 ext4 100 40 60 40% /
",
    )
    .unwrap();
    assert_eq!(usage.available_bytes, 60);
    assert_eq!(usage.usage_percent, 40);
}
