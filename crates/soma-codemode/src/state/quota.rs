#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceQuota {
    pub max_bytes: u64,
}

impl Default for WorkspaceQuota {
    fn default() -> Self {
        Self {
            max_bytes: 32 * 1024 * 1024,
        }
    }
}

impl WorkspaceQuota {
    pub fn check(self, used: u64, new_bytes: u64) -> bool {
        used.saturating_add(new_bytes) <= self.max_bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateWorkspaceLimits {
    pub max_file_bytes: usize,
    pub max_total_bytes: u64,
    pub max_entries: u64,
    pub max_result_bytes: usize,
}

impl Default for StateWorkspaceLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            max_entries: 10_000,
            max_result_bytes: 1024 * 1024,
        }
    }
}
