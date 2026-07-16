use super::job_guard::JobGuard;

#[test]
fn job_guard_constructs_on_all_platforms() {
    let _guard = JobGuard::new(None);
}
