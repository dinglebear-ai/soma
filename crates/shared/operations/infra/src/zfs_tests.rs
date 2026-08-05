use soma_fleet::{HostEndpoint, HostId};

use super::*;

fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}

#[test]
fn requests_validate_targets_types_and_limits() {
    let deadline = Timestamp::from_unix_millis(100);
    let pools = ZfsPoolRequest::new(deadline).with_pool("tank").unwrap();
    assert_eq!(pools.pool(), Some("tank"));
    assert!(ZfsPoolRequest::new(deadline).with_pool("--help").is_err());
    assert!(
        ZfsDatasetRequest::new(deadline)
            .with_pool("tank/apps")
            .unwrap()
            .with_type(ZfsDatasetType::Filesystem)
            .recursive(true)
            .is_recursive()
    );
    assert_eq!(ZfsDatasetType::Snapshot.as_arg(), "snapshot");
    assert!(ZfsSnapshotRequest::new(deadline).with_limit(0).is_err());
    assert!(ZfsSnapshotRequest::new(deadline).with_limit(5001).is_err());
    assert_eq!(
        ZfsSnapshotRequest::new(deadline)
            .with_pool("tank")
            .unwrap()
            .with_dataset("tank/apps@daily")
            .unwrap()
            .dataset(),
        Some("tank/apps@daily")
    );
}

#[test]
fn parser_builds_column_keyed_rows_and_preserves_last_field_spaces() {
    let raw = concat!(
        "NAME USED AVAIL REFER MOUNTPOINT
",
        "tank 10G 90G 96K /tank
",
        "tank/apps 2G 90G 2G /tank/apps with spaces
",
    );
    let table = parse_zfs_table(&host(), raw, None).unwrap();
    assert_eq!(
        table.columns,
        vec!["NAME", "USED", "AVAIL", "REFER", "MOUNTPOINT"]
    );
    assert_eq!(table.rows[1]["NAME"], "tank/apps");
    assert_eq!(table.rows[1]["MOUNTPOINT"], "/tank/apps with spaces");
}

#[test]
fn parser_rejects_short_rows_and_reports_truncation() {
    assert!(
        parse_zfs_table(
            &host(),
            "NAME USED
tank
",
            None
        )
        .is_err()
    );
    let table = parse_zfs_table(
        &host(),
        "NAME USED
tank 1G
backup 2G
",
        Some(1),
    )
    .unwrap();
    assert_eq!(table.rows.len(), 1);
    assert!(table.truncated);
}
