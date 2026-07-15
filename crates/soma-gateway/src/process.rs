//! Process hygiene for stdio upstreams.

pub mod guard;
pub mod stderr;
pub mod stdio;
pub mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildCleanupPolicy {
    KillProcessGroup,
    KillWindowsJob,
}

#[cfg(unix)]
#[must_use]
pub fn default_cleanup_policy() -> ChildCleanupPolicy {
    ChildCleanupPolicy::KillProcessGroup
}

#[cfg(windows)]
#[must_use]
pub fn default_cleanup_policy() -> ChildCleanupPolicy {
    ChildCleanupPolicy::KillWindowsJob
}

#[cfg(not(any(unix, windows)))]
#[must_use]
pub fn default_cleanup_policy() -> ChildCleanupPolicy {
    ChildCleanupPolicy::KillProcessGroup
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
