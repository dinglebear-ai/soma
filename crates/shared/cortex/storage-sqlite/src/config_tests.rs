use super::*;

#[test]
fn storage_defaults_and_derived_sqlite_values_match_donor_contract() {
    let config = StorageConfig::default();
    assert_eq!(config.pool_size, 8);
    assert_eq!(config.retention_days, 90);
    assert_eq!(
        config.sqlite_page_cache_kib_per_connection().unwrap(),
        -16_384
    );
    assert_eq!(config.sqlite_mmap_bytes(), 256 * 1024 * 1024);
    assert_eq!(config.wal_checkpoint_threshold_bytes(), 256 * 1024 * 1024);
}

#[test]
fn page_cache_conversion_rejects_values_that_do_not_fit_sqlite_i64() {
    let config = StorageConfig {
        sqlite_page_cache_mb: u64::MAX,
        pool_size: 1,
        ..StorageConfig::default()
    };
    assert!(config.sqlite_page_cache_kib_per_connection().is_err());
}
