use super::*;

#[test]
fn docker_time_bounds_are_checked_before_query_construction() {
    assert_eq!(to_i32_time("since", None).unwrap(), 0);
    assert_eq!(to_i32_time("since", Some(100)).unwrap(), 100);
    assert!(to_i32_time("since", Some(i64::MAX)).is_err());
}
