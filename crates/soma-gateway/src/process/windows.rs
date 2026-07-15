#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsJobCleanup {
    pub kill_on_drop: bool,
}

impl Default for WindowsJobCleanup {
    fn default() -> Self {
        Self { kill_on_drop: true }
    }
}

#[must_use]
pub fn job_cleanup_required() -> bool {
    true
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
