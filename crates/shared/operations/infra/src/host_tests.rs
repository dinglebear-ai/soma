use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandOutput, FleetResult, HostEndpoint};

use super::*;

struct MockExecutor {
    outputs: BTreeMap<String, CommandOutput>,
    calls: Mutex<Vec<String>>,
}

impl MockExecutor {
    fn standard() -> Self {
        let mut outputs = BTreeMap::new();
        outputs.insert("hostname".into(), ok("dookie\n"));
        outputs.insert("uname -s".into(), ok("Linux\n"));
        outputs.insert("uname -r".into(), ok("7.0.0-test\n"));
        outputs.insert("uname -m".into(), ok("x86_64\n"));
        outputs.insert("cat /proc/uptime".into(), ok("123.50 456.00\n"));
        outputs.insert(
            "cat /proc/meminfo".into(),
            ok("MemTotal: 1000 kB\nMemAvailable: 250 kB\n"),
        );
        outputs.insert("cat /proc/loadavg".into(), ok("1.25 2.50 3.75 1/100 42\n"));
        Self {
            outputs,
            calls: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl CommandExecutor for MockExecutor {
    async fn execute(
        &self,
        _host: &HostRecord,
        request: &CommandRequest,
        cancellation: &CancellationToken,
    ) -> FleetResult<CommandOutput> {
        if cancellation.is_cancelled() {
            return Err(soma_fleet::FleetError::Cancelled);
        }
        let key = std::iter::once(request.program())
            .chain(request.args().iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        self.calls.lock().unwrap().push(key.clone());
        Ok(self.outputs.get(&key).cloned().unwrap())
    }
}

fn ok(text: &str) -> CommandOutput {
    CommandOutput::new(text.as_bytes().to_vec(), Vec::new(), Some(0), false)
}

fn host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

fn request() -> HostInspectRequest {
    HostInspectRequest::new(Timestamp::from_unix_millis(10_000))
}

#[tokio::test(flavor = "current_thread")]
async fn command_inspector_returns_typed_host_snapshot() {
    let executor = Arc::new(MockExecutor::standard());
    let inspector = LinuxCommandHostInspector::new(Arc::clone(&executor));
    let result = inspector
        .inspect(&host(), request(), &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(result.host.as_str(), "dookie");
    assert_eq!(result.identity.hostname, "dookie");
    assert_eq!(result.identity.operating_system, "Linux");
    assert_eq!(result.identity.kernel_release, "7.0.0-test");
    assert_eq!(result.identity.architecture, "x86_64");
    assert_eq!(result.uptime_seconds, 123.5);
    assert_eq!(result.memory.total_bytes, 1_024_000);
    assert_eq!(result.memory.available_bytes, 256_000);
    assert_eq!(result.memory.usage_percent, 75);
    assert_eq!(
        result.load,
        HostLoadAverage {
            one: 1.25,
            five: 2.5,
            fifteen: 3.75
        }
    );
    assert_eq!(executor.calls.lock().unwrap().len(), 7);
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_propagates_before_collection() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let result = LinuxCommandHostInspector::new(Arc::new(MockExecutor::standard()))
        .inspect(&host(), request(), &cancellation)
        .await;
    assert!(matches!(
        result,
        Err(InfraError::Fleet(soma_fleet::FleetError::Cancelled))
    ));
}

#[test]
fn parsers_fail_closed_on_missing_or_invalid_values() {
    assert!(parse_uptime("nope").is_err());
    assert!(parse_meminfo("MemTotal: 10 kB\n").is_err());
    assert!(parse_meminfo("MemTotal: 1 kB\nMemAvailable: 2 kB\n").is_err());
    assert!(parse_meminfo("MemTotal: 10 MB\nMemAvailable: 2 MB\n").is_err());
    assert!(parse_meminfo("MemTotal: 0 kB\nMemAvailable: 0 kB\n").is_err());
    assert!(parse_loadavg("1 2").is_err());
    assert!(parse_loadavg("1 NaN 3").is_err());
}

#[test]
fn nonzero_and_truncated_outputs_are_rejected() {
    let host = host();
    assert!(matches!(
        checked_text(
            &host,
            CommandOutput::new(Vec::new(), b"bad".to_vec(), Some(1), false)
        ),
        Err(InfraError::CommandFailed { .. })
    ));
    assert!(matches!(
        checked_text(
            &host,
            CommandOutput::new(b"partial".to_vec(), Vec::new(), Some(0), true)
        ),
        Err(InfraError::Parse { .. })
    ));
}
