use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::*;
use crate::HostEndpoint;

fn targets(count: usize) -> Vec<HostRecord> {
    (0..count)
        .map(|index| {
            HostRecord::new(
                HostId::new(format!("host{index}")).unwrap(),
                HostEndpoint::Local,
            )
        })
        .collect()
}

#[test]
fn policy_rejects_unbounded_or_zero_time_execution() {
    assert_eq!(
        FanoutPolicy::new(0, Duration::from_secs(1)),
        Err(FanoutPolicyError::ZeroConcurrency)
    );
    assert_eq!(
        FanoutPolicy::new(1, Duration::ZERO),
        Err(FanoutPolicyError::ZeroTimeout)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn fanout_is_bounded_and_restores_input_order() {
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let scheduler = FanoutScheduler::new(FanoutPolicy::new(2, Duration::from_secs(1)).unwrap());
    let report = scheduler
        .run(targets(6), CancellationToken::new(), {
            let active = Arc::clone(&active);
            let peak = Arc::clone(&peak);
            move |host, _| {
                let active = Arc::clone(&active);
                let peak = Arc::clone(&peak);
                async move {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(current, Ordering::SeqCst);
                    let index = host
                        .id()
                        .as_str()
                        .trim_start_matches("host")
                        .parse::<u64>()
                        .unwrap();
                    tokio::time::sleep(Duration::from_millis((6 - index) * 4)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok::<_, &'static str>(host.id().as_str().to_owned())
                }
            }
        })
        .await;

    assert!(peak.load(Ordering::SeqCst) <= 2);
    assert_eq!(report.success_count(), 6);
    assert!(report.all_succeeded());
    assert_eq!(
        report
            .outcomes()
            .iter()
            .map(|outcome| outcome.host().as_str())
            .collect::<Vec<_>>(),
        vec!["host0", "host1", "host2", "host3", "host4", "host5"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn fanout_classifies_failures_timeouts_and_partial_success() {
    let scheduler = FanoutScheduler::new(FanoutPolicy::new(2, Duration::from_millis(10)).unwrap());
    let report = scheduler
        .run(targets(4), CancellationToken::new(), |host, _| async move {
            match host.id().as_str() {
                "host1" => Err("driver failed"),
                "host2" => {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    Ok("late")
                }
                _ => Ok("ok"),
            }
        })
        .await;

    assert_eq!(report.success_count(), 2);
    assert_eq!(report.failure_count(), 1);
    assert_eq!(report.timed_out_count(), 1);
    assert_eq!(report.cancelled_count(), 0);
    assert!(!report.all_succeeded());
    assert!(matches!(
        report.outcomes()[1].kind(),
        TargetOutcomeKind::Failed("driver failed")
    ));
    assert!(matches!(
        report.outcomes()[2].kind(),
        TargetOutcomeKind::TimedOut
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_accounts_for_inflight_and_queued_targets() {
    let cancellation = CancellationToken::new();
    let trigger = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        trigger.cancel();
    });
    let scheduler = FanoutScheduler::new(FanoutPolicy::new(2, Duration::from_secs(1)).unwrap());
    let report = scheduler
        .run(targets(5), cancellation, |_host, _| async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok::<_, ()>(())
        })
        .await;

    assert_eq!(report.outcomes().len(), 5);
    assert_eq!(report.cancelled_count(), 5);
    assert_eq!(report.success_count(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn payload_fanout_preserves_duplicate_host_requests_by_index() {
    let host = HostRecord::new(HostId::new("same").unwrap(), HostEndpoint::Local);
    let scheduler = FanoutScheduler::new(FanoutPolicy::new(2, Duration::from_secs(1)).unwrap());
    let report = scheduler
        .run_with_payload(
            vec![(host.clone(), "first"), (host, "second")],
            CancellationToken::new(),
            |_host, payload, _| async move { Ok::<_, ()>(payload) },
        )
        .await;

    assert_eq!(report.outcomes().len(), 2);
    assert!(matches!(
        report.outcomes()[0].kind(),
        TargetOutcomeKind::Succeeded("first")
    ));
    assert!(matches!(
        report.outcomes()[1].kind(),
        TargetOutcomeKind::Succeeded("second")
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn fanout_report_can_be_consumed_without_cloning_results() {
    let scheduler = FanoutScheduler::new(FanoutPolicy::new(1, Duration::from_secs(1)).unwrap());
    let report = scheduler
        .run(
            targets(1),
            CancellationToken::new(),
            |_host, _| async move { Ok::<_, String>(String::from("owned")) },
        )
        .await;
    let outcomes = report.into_outcomes();
    assert!(matches!(
        &outcomes[0].kind,
        TargetOutcomeKind::Succeeded(value) if value == "owned"
    ));
}
