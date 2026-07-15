use super::*;
use std::io::Cursor;

#[test]
fn drains_and_caps_large_stderr_without_unbounded_output() {
    let input = vec![b'x'; 10_000];
    let drained = drain_stderr_with_cap(Cursor::new(input), 128).unwrap();
    assert_eq!(drained.text.len(), 128);
    assert!(drained.truncated);
}

#[test]
fn preserves_small_stderr() {
    let drained = drain_stderr_with_cap(Cursor::new("hello"), 128).unwrap();
    assert_eq!(drained.text, "hello");
    assert!(!drained.truncated);
}
