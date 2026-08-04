use super::*;

#[test]
fn output_bounding_retains_prefix_and_reports_truncation() {
    let mut output = b"ab".to_vec();
    assert!(append_bounded(&mut output, b"cdef", 4));
    assert_eq!(output, b"abcd");
    assert!(append_bounded(&mut output, b"z", 4));
    assert_eq!(output, b"abcd");
}

#[test]
fn pre_and_post_start_deadlines_preserve_different_send_states() {
    let deadline = Timestamp::from_unix_millis(Timestamp::now().unix_millis() - 1);
    assert_eq!(
        remaining(deadline, MutationSendState::NotSent)
            .unwrap_err()
            .send_state(),
        MutationSendState::NotSent
    );
    assert_eq!(
        remaining(deadline, MutationSendState::Unknown)
            .unwrap_err()
            .send_state(),
        MutationSendState::Unknown
    );
}
