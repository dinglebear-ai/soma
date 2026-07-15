use super::normalize::normalize_user_code;

#[test]
fn wraps_loose_expression() {
    assert_eq!(
        normalize_user_code("1 + 2"),
        "async () => {\nreturn (1 + 2)\n}"
    );
}

#[test]
fn strips_markdown_fence() {
    let normalized = normalize_user_code("```js\nasync () => 42\n```");
    assert_eq!(normalized, "async () => 42");
}

#[test]
fn function_declaration_is_invoked() {
    let normalized = normalize_user_code("async function main() { return 1; }");
    assert!(normalized.contains("return main();"));
}
