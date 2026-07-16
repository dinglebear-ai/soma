pub const ADMIN_SCOPE: &str = "soma:admin";

pub fn has_admin_scope(scopes: &[String]) -> bool {
    scopes.iter().any(|scope| scope == ADMIN_SCOPE)
}

#[cfg(test)]
#[path = "scopes_tests.rs"]
mod tests;
