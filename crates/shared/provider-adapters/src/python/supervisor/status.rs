use std::sync::atomic::Ordering;

use super::{PythonWorkerStatus, PythonWorkerSupervisor};

impl PythonWorkerSupervisor {
    /// Returns a bounded, redacted snapshot without executing provider code.
    #[must_use]
    pub fn status(&self) -> PythonWorkerStatus {
        let restart_count = self.current_restart_count();
        let running = self.worker_running();
        let logs = self
            .logs
            .lock()
            .expect("Python worker log lock should not be poisoned");
        PythonWorkerStatus {
            provider_source: self.identity.path.clone(),
            generation_id: self.identity.generation_id.clone(),
            running,
            accepting: self.accepting.load(Ordering::Acquire),
            busy: self.busy.load(Ordering::Acquire),
            quarantined: self.quarantined.load(Ordering::Acquire),
            restart_count,
            logs: logs.entries.iter().cloned().collect(),
            execution_profile: self.host.profile(),
            host_audit: self.host.audit_events(),
        }
    }
}
