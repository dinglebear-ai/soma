use super::disposition::RunnerDisposition;

#[test]
fn disposition_recycles_after_threshold() {
    assert_eq!(
        RunnerDisposition::from_success_count(1, 2),
        RunnerDisposition::Reuse
    );
    assert_eq!(
        RunnerDisposition::from_success_count(2, 2),
        RunnerDisposition::Recycle
    );
}
