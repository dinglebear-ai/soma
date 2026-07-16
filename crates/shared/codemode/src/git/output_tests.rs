use super::output::cap_output;

#[test]
fn output_is_capped() {
    assert_eq!(cap_output(b"abcdef", 3), "abc");
}
