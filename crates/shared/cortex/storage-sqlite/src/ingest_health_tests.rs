use super::*;
use crate::{StorageConfig, init_pool};

#[test]
fn health_groups_recent_rows_by_normalized_source_kind() {
    let dir = tempfile::tempdir().unwrap();
    let config = StorageConfig {
        db_path: dir.path().join("health.db"),
        pool_size: 1,
        wal_mode: false,
        ..StorageConfig::default()
    };
    let pool = init_pool(&config).unwrap();
    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO logs(timestamp, hostname, severity, message, raw, received_at, source_ip, metadata_json) VALUES (?1,'nas','info','a','a',?1,'docker://nas/c/stdout',NULL)",
        ["2026-08-18T12:00:00Z"],
    ).unwrap();
    conn.execute(
        "INSERT INTO logs(timestamp, hostname, severity, message, raw, received_at, source_ip, metadata_json) VALUES (?1,'nas','info','b','b',?1,'198.51.100.1:514',?2)",
        rusqlite::params!["2026-08-18T11:30:00Z", r#"{"source_kind":"syslog-udp"}"#],
    ).unwrap();
    drop(conn);

    let rows = ingest_source_kind_health(
        &pool,
        "2026-08-18T12:15:00Z",
        "2026-08-18T12:00:00Z",
        "2026-08-18T11:15:00Z",
        "2026-08-17T12:15:00Z",
    )
    .unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| r.source_kind.as_str())
            .collect::<Vec<_>>(),
        vec!["docker-stream", "syslog-udp"]
    );
    assert_eq!(rows[0].last_15m, 1);
    assert_eq!(rows[1].last_1h, 1);
}
