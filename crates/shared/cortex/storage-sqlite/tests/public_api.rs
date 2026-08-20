use cortex_storage_sqlite::{
    KNOWN_SCHEMA_VERSION, LogBatchEntry, StorageConfig, fetch_patterns, init_pool,
    insert_logs_batch, read_schema_version_info, tail_logs,
};

#[test]
fn independent_consumer_initializes_schema_and_round_trips_logs() {
    let dir = tempfile::tempdir().unwrap();
    let config = StorageConfig {
        db_path: dir.path().join("consumer.db"),
        pool_size: 1,
        wal_mode: false,
        ..StorageConfig::default()
    };

    let pool = init_pool(&config).unwrap();
    let schema = read_schema_version_info(&pool).unwrap();
    assert_eq!(KNOWN_SCHEMA_VERSION, 47);
    assert_eq!(schema.version, KNOWN_SCHEMA_VERSION);
    assert_eq!(schema.known_version, KNOWN_SCHEMA_VERSION);

    let entry = LogBatchEntry {
        timestamp: "2026-08-18T12:00:00Z".into(),
        hostname: "dookie".into(),
        facility: Some("daemon".into()),
        severity: "info".into(),
        app_name: Some("consumer-test".into()),
        process_id: Some("1".into()),
        message: "storage consumer round trip".into(),
        raw: "storage consumer round trip".into(),
        source_ip: "127.0.0.1:514".into(),
        docker_checkpoint: None,
        ai_tool: None,
        ai_project: None,
        ai_session_id: None,
        ai_transcript_path: None,
        metadata_json: None,
        http_status: None,
        auth_outcome: None,
        dns_blocked: None,
        event_action: None,
        parse_error: None,
    };
    insert_logs_batch(&pool, &[entry]).unwrap();

    let rows = tail_logs(&pool, Some("dookie"), None, None, None, 10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message, "storage consumer round trip");
    assert_eq!(rows[0].hostname, "dookie");

    let (patterns, scanned, truncated) =
        fetch_patterns(&pool, None, None, None, None, None, 100, 10).unwrap();
    assert_eq!(scanned, 1);
    assert!(!truncated);
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].count, 1);
}
