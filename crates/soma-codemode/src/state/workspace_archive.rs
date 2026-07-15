use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceArchiveMeta {
    pub files: usize,
    pub bytes: u64,
}
