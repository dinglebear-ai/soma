use std::{fs, sync::atomic::Ordering};

use sha2::{Digest, Sha256};

use super::{
    PythonInterpreter, PythonSupervisorConfig, PythonWorkerIdentity, PythonWorkerSupervisor,
};

#[test]
fn cooperative_cancellation_is_reported_before_forced_termination() {
    let temp = tempfile::tempdir().expect("tempdir");
    let provider = temp.path().join("cancel_failure.py");
    fs::write(&provider, "PROVIDER = {'name': 'cancel-failure'}\n").expect("provider");
    let source_digest = Sha256::digest(fs::read(&provider).expect("provider source"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let supervisor = PythonWorkerSupervisor::new(
        PythonWorkerIdentity {
            path: provider,
            generation_id: "cancel-test-generation".to_owned(),
            worker_group: "cancel-test-generation".to_owned(),
            source_digest,
            catalog_fingerprint: String::new(),
        },
        PythonInterpreter::Ambient,
        PythonSupervisorConfig::default(),
    );
    supervisor.busy.store(true, Ordering::Release);
    supervisor.active_pid.store(42, Ordering::Release);

    assert!(supervisor.cancel_active_with(|_| false));
    assert_eq!(supervisor.active_pid.load(Ordering::Acquire), 42);
    assert_eq!(supervisor.cancel_epoch.load(Ordering::Acquire), 1);
    assert!(supervisor.discard_worker.load(Ordering::Acquire));
}
