use super::*;

#[test]
fn mcp_identity_prefers_verified_email_for_display() {
    let actor = RequestActor::mcp_identity(Some("sub-123".into()), Some("me@example.com".into()));
    assert_eq!(actor.surface, "mcp");
    assert_eq!(actor.display, "me@example.com");
    assert_eq!(actor.subject.as_deref(), Some("sub-123"));
    assert_eq!(actor.email.as_deref(), Some("me@example.com"));
}

#[test]
fn request_actor_wire_shape_matches_donor() {
    let value = serde_json::to_value(RequestActor::api()).unwrap();
    assert_eq!(value, serde_json::json!({"surface":"api","display":"api"}));
}
