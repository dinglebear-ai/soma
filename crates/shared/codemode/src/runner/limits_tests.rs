use super::limits::*;

#[test]
fn runner_limits_are_bounded() {
    let memory_limit = MEMORY_LIMIT_BYTES;
    let max_line = MAX_STDIO_LINE_BYTES;
    assert!(memory_limit <= 64 * 1024 * 1024);
    assert!(max_line >= 1024);
}
