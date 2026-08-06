use super::*;

#[test]
fn transfer_executor_is_available_as_a_separate_impl_surface() {
    let _ = std::mem::size_of::<SynapseMutationRuntime>();
}
