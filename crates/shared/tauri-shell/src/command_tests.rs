use super::TauriResultExt;

#[test]
fn ok_result_passes_through_unchanged() {
    let result: Result<u32, std::io::Error> = Ok(7);
    assert_eq!(result.command_result(), Ok(7));
}

#[test]
fn err_result_is_stringified_via_display() {
    let result: Result<u32, std::io::Error> =
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
    assert_eq!(result.command_result(), Err("missing".to_string()));
}
