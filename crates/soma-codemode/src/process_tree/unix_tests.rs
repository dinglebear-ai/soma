#[test]
fn unix_terminator_symbol_exists() {
    let _fn_ptr: fn(u32) = super::unix::terminate_process_tree;
}
