#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerDisposition {
    Reuse,
    Recycle,
    Evict,
}

impl RunnerDisposition {
    pub fn from_success_count(success_count: u64, recycle_after: u64) -> Self {
        if success_count >= recycle_after.max(1) {
            Self::Recycle
        } else {
            Self::Reuse
        }
    }
}
