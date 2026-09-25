use soma_fleet::{HostEndpoint, HostId};

use super::*;

fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}

#[test]
fn requests_validate_filters_limits_and_sort_arguments() {
    let deadline = Timestamp::from_unix_millis(100);
    let request = ProcessListRequest::new(deadline)
        .with_sort(ProcessSort::Memory)
        .with_grep("soma")
        .unwrap()
        .with_user("devuser")
        .unwrap()
        .with_limit(25)
        .unwrap();
    assert_eq!(request.sort(), ProcessSort::Memory);
    assert_eq!(request.sort().ps_argument(), "-mem");
    assert_eq!(request.grep(), Some("soma"));
    assert_eq!(request.user(), Some("devuser"));
    assert_eq!(request.limit(), 25);
    assert!(ProcessListRequest::new(deadline).with_limit(0).is_err());
    assert!(ProcessListRequest::new(deadline).with_limit(501).is_err());
    assert!(
        ProcessListRequest::new(deadline)
            .with_grep("bad\0value")
            .is_err()
    );
}

#[test]
fn process_rows_are_typed_filtered_and_bounded() {
    let raw = concat!(
        "root 1 0.1 0.2 1000 500 ? Ss 10:00 0:01 /sbin/init
",
        "devuser 22 10.5 3.5 2000 1000 pts/0 Sl 10:01 1:02 /usr/bin/soma serve --port 40060
",
        "devuser 23 5.0 1.0 1500 700 pts/1 S 10:02 0:10 cargo test
",
    );
    let request = ProcessListRequest::new(Timestamp::from_unix_millis(100))
        .with_user("devuser")
        .unwrap()
        .with_grep("soma")
        .unwrap()
        .with_limit(1)
        .unwrap();
    let snapshot = parse_process_rows(&host(), &request, raw).unwrap();
    assert_eq!(snapshot.rows.len(), 1);
    assert!(!snapshot.truncated);
    assert_eq!(snapshot.rows[0].pid, 22);
    assert_eq!(snapshot.rows[0].cpu_percent, 10.5);
    assert_eq!(snapshot.rows[0].command, "/usr/bin/soma serve --port 40060");
}

#[test]
fn parser_rejects_malformed_rows_and_reports_truncation() {
    let request = ProcessListRequest::new(Timestamp::from_unix_millis(100))
        .with_limit(1)
        .unwrap();
    assert!(parse_process_rows(&host(), &request, "bad row").is_err());
    let raw = concat!(
        "root 1 0.1 0.2 1000 500 ? Ss 10:00 0:01 init
",
        "root 2 0.2 0.3 1000 500 ? S 10:01 0:02 kthreadd
",
    );
    assert!(
        parse_process_rows(&host(), &request, raw)
            .unwrap()
            .truncated
    );
}
