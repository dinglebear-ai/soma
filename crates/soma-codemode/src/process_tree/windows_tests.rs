#[test]
fn windows_terminator_symbol_exists() {
    let _fn_ptr: fn(u32) = super::windows::terminate_process_tree;
}
