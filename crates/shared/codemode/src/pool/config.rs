pub const POOL_SIZE_ENV: &str = "SOMA_CODE_MODE_POOL_SIZE";
pub const RECYCLE_AFTER_ENV: &str = "SOMA_CODE_MODE_POOL_RECYCLE_AFTER";
pub const MAX_OVERFLOW_ENV: &str = "SOMA_CODE_MODE_POOL_MAX_OVERFLOW";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolConfig {
    pub size: usize,
    pub recycle_after: u64,
    pub max_overflow: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            size: 2,
            recycle_after: 100,
            max_overflow: 8,
        }
    }
}

impl PoolConfig {
    pub fn from_env() -> Self {
        Self {
            size: read_usize(POOL_SIZE_ENV, 2).min(16),
            recycle_after: read_u64(RECYCLE_AFTER_ENV, 100).max(1),
            max_overflow: read_usize(MAX_OVERFLOW_ENV, 8).min(64),
        }
    }

    pub fn is_disabled(self) -> bool {
        self.size == 0
    }
}

fn read_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

fn read_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}
