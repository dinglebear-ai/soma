#[test]
fn process_tree_entrypoint_is_callable() {
    let _fn_ptr: fn(u32) = super::process_tree::terminate_process_tree;
}
