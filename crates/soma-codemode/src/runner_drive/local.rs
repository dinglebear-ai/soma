pub fn is_local_provider(id: &str) -> bool {
    id.starts_with("state::") || id.starts_with("git::") || {
        #[cfg(feature = "openapi")]
        {
            id.starts_with("openapi::")
        }
        #[cfg(not(feature = "openapi"))]
        {
            let _ = id;
            false
        }
    }
}
