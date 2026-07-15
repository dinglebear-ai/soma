pub const INTERNAL_NAMESPACE: &str = "__soma_internal";

pub fn is_internal_call(id: &str) -> bool {
    id.split_once("::")
        .is_some_and(|(namespace, _)| namespace == INTERNAL_NAMESPACE)
}
