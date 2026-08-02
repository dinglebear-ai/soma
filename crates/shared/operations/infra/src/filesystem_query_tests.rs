use super::*;

#[test]
fn query_requests_are_closed_and_bounded() {
    let deadline = Timestamp::from_unix_millis(100);
    assert!(PathReadRequest::new(deadline).with_tree(0).is_err());
    assert_eq!(
        PathReadRequest::new(deadline).with_tree(4).unwrap().depth(),
        4
    );
    assert!(FileFindRequest::new("--help", deadline).is_err());
    let find = FileFindRequest::new("*.log", deadline)
        .unwrap()
        .with_depth(5)
        .unwrap()
        .with_limit(25)
        .unwrap();
    assert_eq!(find.pattern(), "*.log");
    assert_eq!(find.limit(), 25);
    assert!(FileTailRequest::new(deadline).with_lines(0).is_err());
    assert_eq!(
        FileTailRequest::new(deadline)
            .with_lines(250)
            .unwrap()
            .lines(),
        250
    );
}
