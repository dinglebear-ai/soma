//! SQLite-specific storage configuration.
//!
//! These fields are extracted from Cortex's product-level configuration so the
//! persistence adapter can be configured without depending on the runtime.

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration required by the Cortex SQLite adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub db_path: PathBuf,
    pub pool_size: u32,
    pub sqlite_page_cache_mb: u64,
    pub sqlite_mmap_mb: u64,
    pub heavy_read_concurrency: usize,
    pub wal_checkpoint_mb: u64,
    pub retention_days: u32,
    pub wal_mode: bool,
    pub max_db_size_mb: u64,
    pub recovery_db_size_mb: u64,
    pub min_free_disk_mb: u64,
    pub recovery_free_disk_mb: u64,
    pub cleanup_interval_secs: u64,
    pub cleanup_chunk_size: usize,
    pub err_floor_window_hours: u64,
    pub err_floor_per_source_cap: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("/data/cortex.db"),
            pool_size: 8,
            sqlite_page_cache_mb: 128,
            sqlite_mmap_mb: 256,
            heavy_read_concurrency: 1,
            wal_checkpoint_mb: 256,
            retention_days: 90,
            wal_mode: true,
            max_db_size_mb: 1024,
            recovery_db_size_mb: 900,
            min_free_disk_mb: 0,
            recovery_free_disk_mb: 0,
            cleanup_interval_secs: 60,
            cleanup_chunk_size: 2_000,
            err_floor_window_hours: 24,
            err_floor_per_source_cap: 10_000,
        }
    }
}

impl StorageConfig {
    pub fn sqlite_page_cache_kib_per_connection(&self) -> anyhow::Result<i64> {
        let pool_size = u64::from(self.pool_size.max(1));
        let total_kib = self
            .sqlite_page_cache_mb
            .checked_mul(1024)
            .context("storage.sqlite_page_cache_mb is too large")?;
        let per_conn = (total_kib / pool_size).max(1);
        i64::try_from(per_conn)
            .context(
                "storage.sqlite_page_cache_mb is too large; derived cache_size must fit in i64",
            )
            .map(|value| -value)
    }

    pub fn sqlite_mmap_bytes_i64(&self) -> anyhow::Result<i64> {
        i64::try_from(self.sqlite_mmap_bytes())
            .context("storage.sqlite_mmap_mb is too large; derived mmap_size must fit in i64")
    }

    #[must_use]
    pub fn sqlite_mmap_bytes(&self) -> u64 {
        self.sqlite_mmap_mb.saturating_mul(1024 * 1024)
    }

    #[must_use]
    pub fn wal_checkpoint_threshold_bytes(&self) -> u64 {
        self.wal_checkpoint_mb.saturating_mul(1024 * 1024)
    }

    #[cfg(test)]
    pub(crate) fn for_test(db_path: PathBuf) -> Self {
        Self {
            db_path,
            pool_size: 1,
            wal_mode: false,
            cleanup_chunk_size: 1,
            ..Self::default()
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
