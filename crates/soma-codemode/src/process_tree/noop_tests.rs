#[test]
fn noop_terminator_symbol_exists() {
    let _fn_ptr: fn(u32) = super::noop::terminate_process_tree;
}
