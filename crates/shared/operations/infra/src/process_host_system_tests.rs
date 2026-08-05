use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{
    CommandExecutor, CommandOutput, CommandRequest, FleetResult, HostEndpoint, HostId,
};

use super::*;
use crate::{PortProtocol, ServiceListRequest};

struct MockExecutor {
    outputs: Mutex<VecDeque<CommandOutput>>,
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

#[async_trait]
impl CommandExecutor for MockExecutor {
    async fn execute(
        &self,
        _host: &HostRecord,
        request: &CommandRequest,
        _cancellation: &CancellationToken,
    ) -> FleetResult<CommandOutput> {
        self.calls
            .lock()
            .unwrap()
            .push((request.program().to_owned(), request.args().to_vec()));
        Ok(self.outputs.lock().unwrap().pop_front().unwrap())
    }
}

fn ok(value: &str) -> CommandOutput {
    CommandOutput::new(value.as_bytes().to_vec(), Vec::new(), Some(0), false)
}
fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}
fn deadline() -> Timestamp {
    Timestamp::from_unix_millis(10_000)
}

#[tokio::test(flavor = "current_thread")]
async fn command_driver_parses_all_host_system_reads() {
    let executor = Arc::new(MockExecutor {
        outputs: Mutex::new(VecDeque::from([
            ok("sshd.service loaded active running SSH daemon
"),
            ok(r#"[{"ifindex":2,"ifname":"eth0","operstate":"UP","addr_info":[]}]"#),
            ok(r#"{"filesystems":[{"target":"/","source":"/dev/sda1","fstype":"ext4"}]}"#),
            ok("tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:sshd
"),
            ok("Filesystem Type 1B-blocks Used Available Use% Mounted on
/dev/sda1 ext4 100 40 60 40% /
"),
        ])),
        calls: Mutex::new(Vec::new()),
    });
    let driver = CommandHostSystemInspector::new(Arc::clone(&executor));
    let cancellation = CancellationToken::new();
    assert_eq!(
        driver
            .services(&host(), &ServiceListRequest::new(deadline()), &cancellation)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        driver
            .network(&host(), deadline(), &cancellation)
            .await
            .unwrap()[0]
            .name,
        "eth0"
    );
    assert_eq!(
        driver
            .mounts(&host(), deadline(), &cancellation)
            .await
            .unwrap()[0]
            .target,
        "/"
    );
    assert_eq!(
        driver
            .ports(
                &host(),
                &PortListRequest::new(deadline()).with_protocol(PortProtocol::Tcp),
                &cancellation
            )
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        driver
            .filesystem_usage(&host(), Some("/"), deadline(), &cancellation)
            .await
            .unwrap()
            .usage_percent,
        40
    );
    let calls = executor.calls.lock().unwrap();
    assert_eq!(calls[0].0, "systemctl");
    assert_eq!(calls[1].1, vec!["-j", "address"]);
    assert_eq!(calls[3].1.last().map(String::as_str), Some("-t"));
}

#[tokio::test(flavor = "current_thread")]
async fn doctor_records_failures_instead_of_hiding_them() {
    let failed = CommandOutput::new(Vec::new(), b"missing".to_vec(), Some(1), false);
    let executor = Arc::new(MockExecutor {
        outputs: Mutex::new(VecDeque::from([failed.clone(), failed.clone(), failed])),
        calls: Mutex::new(Vec::new()),
    });
    let driver = CommandHostSystemInspector::new(executor);
    let report = driver
        .doctor(&host(), deadline(), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(report.overall, "degraded");
    assert!(report.checks.iter().all(|check| !check.ok));
}
