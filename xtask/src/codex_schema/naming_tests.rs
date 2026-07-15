use super::*;

#[test]
fn method_to_pascal_handles_slash_and_underscore_segments() {
    assert_eq!(method_to_pascal("initialize"), "Initialize");
    assert_eq!(method_to_pascal("thread/start"), "ThreadStart");
    assert_eq!(
        method_to_pascal("item/commandExecution/requestApproval"),
        "ItemCommandExecutionRequestApproval"
    );
    assert_eq!(
        method_to_pascal("config/mcpServer/reload"),
        "ConfigMcpServerReload"
    );
    assert_eq!(method_to_pascal("account_read"), "AccountRead");
}

#[test]
fn camel_tokens_splits_camel_case_without_acronyms() {
    assert_eq!(camel_tokens("mcpServer"), vec!["mcp", "server"]);
    assert_eq!(
        camel_tokens("requestUserInput"),
        vec!["request", "user", "input"]
    );
    assert_eq!(camel_tokens("reload"), vec!["reload"]);
}

#[test]
fn camel_tokens_keeps_acronym_runs_together() {
    // "HTTPServer": the greedy uppercase run backtracks to leave the next
    // capital for the following segment, producing ["http", "server"] - not
    // ["h", "t", "t", "p", "server"] or ["httpserver"].
    assert_eq!(camel_tokens("HTTPServer"), vec!["http", "server"]);
    assert_eq!(
        camel_tokens("parseHTTPResponseBody"),
        vec!["parse", "http", "response", "body"]
    );
    assert_eq!(camel_tokens("ID"), vec!["id"]);
}

#[test]
fn method_to_snake_fn_matches_known_manifest_entries() {
    // Cross-checked against the committed schema/methods.json.
    assert_eq!(method_to_snake_fn("initialize"), "initialize");
    assert_eq!(method_to_snake_fn("thread/start"), "thread_start");
    assert_eq!(
        method_to_snake_fn("item/tool/requestUserInput"),
        "item_tool_request_user_input"
    );
    assert_eq!(
        method_to_snake_fn("mcpServer/oauth/login"),
        "mcp_server_oauth_login"
    );
    assert_eq!(
        method_to_snake_fn("config/mcpServer/reload"),
        "config_mcp_server_reload"
    );
}

#[test]
fn response_override_wins_over_naming_convention() {
    let mut defs = Map::new();
    defs.insert("GetAccountResponse".to_string(), Value::Bool(true));
    // The naming-convention candidate ("AccountReadResponse") deliberately
    // does NOT exist, to prove the override is what resolves this.
    let resolved = resolve_response("account/read", &defs).unwrap();
    assert_eq!(resolved.as_deref(), Some("GetAccountResponse"));
}

#[test]
fn resolve_response_uses_naming_convention_when_no_override() {
    let mut defs = Map::new();
    defs.insert("ThreadStartResponse".to_string(), Value::Bool(true));
    let resolved = resolve_response("thread/start", &defs).unwrap();
    assert_eq!(resolved.as_deref(), Some("ThreadStartResponse"));
}

#[test]
fn resolve_response_falls_back_to_fuzzy_match() {
    // Mirrors the real "thread/name/set" -> "ThreadSetNameResponse" case:
    // the naming convention alone would guess "ThreadNameSetResponse",
    // which doesn't exist, so it needs the token-subset fuzzy match.
    let mut defs = Map::new();
    defs.insert("ThreadSetNameResponse".to_string(), Value::Bool(true));
    let resolved = resolve_response("thread/name/set", &defs).unwrap();
    assert_eq!(resolved.as_deref(), Some("ThreadSetNameResponse"));
}

#[test]
fn resolve_response_honors_known_void_response_methods() {
    let defs = Map::new();
    let resolved = resolve_response("config/mcpServer/reload", &defs).unwrap();
    assert_eq!(resolved, None);
}

#[test]
fn resolve_response_hard_fails_when_nothing_matches() {
    let defs = Map::new();
    let err = resolve_response("totally/unknown/method", &defs).unwrap_err();
    assert!(err
        .to_string()
        .contains("could not resolve a response type"));
}

#[test]
fn fuzzy_response_match_prefers_shortest_candidate() {
    let names = vec!["ThreadSetNameExtraResponse", "ThreadSetNameResponse"];
    let m = fuzzy_response_match("thread/name/set", names.into_iter()).unwrap();
    assert_eq!(m.as_deref(), Some("ThreadSetNameResponse"));
}

#[test]
fn fuzzy_response_match_requires_full_token_subset() {
    // "account/logout" tokens {account, logout} are NOT a subset of
    // "LoginAccountResponse" tokens {login, account} - must not match.
    let names = vec!["LoginAccountResponse", "LogoutAccountResponse"];
    let m = fuzzy_response_match("account/logout", names.into_iter()).unwrap();
    assert_eq!(m.as_deref(), Some("LogoutAccountResponse"));
}

#[test]
fn fuzzy_response_match_bails_on_a_tie_for_shortest() {
    // Both candidates are the same length and both are token-supersets of
    // "thread/name/set"'s tokens {thread, name, set} - genuinely ambiguous,
    // must not silently pick one.
    let names = vec!["ThreadSetNameAResponse", "ThreadSetNameBResponse"];
    let err = fuzzy_response_match("thread/name/set", names.into_iter()).unwrap_err();
    assert!(err
        .to_string()
        .contains("ambiguous fuzzy response-type match"));
}

#[test]
fn fuzzy_response_match_does_not_bail_when_shortest_is_unique_even_with_longer_candidates() {
    let names = vec![
        "ThreadSetNameResponse",
        "ThreadSetNameWithExtraStuffResponse",
    ];
    let m = fuzzy_response_match("thread/name/set", names.into_iter()).unwrap();
    assert_eq!(m.as_deref(), Some("ThreadSetNameResponse"));
}
