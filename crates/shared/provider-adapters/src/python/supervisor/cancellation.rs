use std::{sync::atomic::Ordering, time::Duration};

use super::{PythonWorkerSupervisor, terminate_process_tree};

impl PythonWorkerSupervisor {
    /// Cooperatively marks the active invocation cancelled, then terminates
    /// its complete process tree after a short grace period. The next
    /// invocation starts a fresh worker.
    pub fn cancel_active(&self) -> bool {
        self.cancel_active_with(terminate_process_tree)
    }

    pub(super) fn cancel_active_with(
        &self,
        terminator: impl FnOnce(Option<u32>) -> bool + Send + 'static,
    ) -> bool {
        if !self.busy.load(Ordering::Acquire) {
            return false;
        }
        let pid = self.active_pid.load(Ordering::Acquire);
        if pid == 0 {
            return false;
        }
        self.host.cancel_invocation();
        self.cancel_epoch.fetch_add(1, Ordering::AcqRel);
        self.discard_worker.store(true, Ordering::Release);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            let _ = terminator(Some(pid));
        });
        true
    }
}
