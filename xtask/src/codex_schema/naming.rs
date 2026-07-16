//! Method-name -> Rust-identifier derivation, ported from
//! `build_combined_schema.py`. Naming here must exactly match what `typify`
//! names the corresponding generated Rust types/enum variants - this was
//! empirically cross-validated against real typify output when this crate
//! was first built, so keep it in lock-step with the Python original rather
//! than "simplifying".

use std::collections::BTreeSet;
use std::sync::OnceLock;

use anyhow::{bail, Result};
use fancy_regex::Regex;
use serde_json::{Map, Value};

/// `PascalCase` derived from a `/`- and `_`-delimited method name, e.g.
/// `"thread/start"` -> `"ThreadStart"`. Only the first character of each
/// segment is uppercased; the rest of the segment is left untouched (so
/// already-camelCase segments like `"mcpServer"` become `"McpServer"`, not
/// `"MCPSERVER"` or `"Mcpserver"`).
pub fn method_to_pascal(method: &str) -> String {
    method
        .split(['/', '_'])
        .filter(|t| !t.is_empty())
        .map(|t| {
            let mut chars = t.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Known irregular method -> response-type-name mappings the naming
/// convention can't derive (see `crates/shared/codex-app-server-client/README.md`
/// for how these were discovered).
pub const RESPONSE_OVERRIDES: &[(&str, &str)] = &[
    ("account/read", "GetAccountResponse"),
    ("account/rateLimits/read", "GetAccountRateLimitsResponse"),
    ("account/usage/read", "GetAccountTokenUsageResponse"),
    (
        "account/workspaceMessages/read",
        "GetWorkspaceMessagesResponse",
    ),
    ("account/login/start", "LoginAccountResponse"),
    (
        "account/sendAddCreditsNudgeEmail",
        "SendAddCreditsNudgeEmailResponse",
    ),
    (
        "account/chatgptAuthTokens/refresh",
        "ChatgptAuthTokensRefreshResponse",
    ),
    ("app/list", "AppsListResponse"),
    ("config/batchWrite", "ConfigWriteResponse"),
    ("config/value/write", "ConfigWriteResponse"),
    (
        "item/commandExecution/requestApproval",
        "CommandExecutionRequestApprovalResponse",
    ),
    (
        "item/fileChange/requestApproval",
        "FileChangeRequestApprovalResponse",
    ),
    (
        "item/permissions/requestApproval",
        "PermissionsRequestApprovalResponse",
    ),
    ("item/tool/call", "DynamicToolCallResponse"),
    ("item/tool/requestUserInput", "ToolRequestUserInputResponse"),
    ("mcpServer/resource/read", "McpResourceReadResponse"),
    (
        "remoteControl/client/list",
        "RemoteControlClientsListResponse",
    ),
    (
        "remoteControl/client/revoke",
        "RemoteControlClientsRevokeResponse",
    ),
    // config/mcpServer/reload has no response payload (params/result are both `undefined`)
];

fn response_override(method: &str) -> Option<&'static str> {
    RESPONSE_OVERRIDES
        .iter()
        .find(|(m, _)| *m == method)
        .map(|(_, r)| *r)
}

/// Methods confirmed (by hand, against the schema) to genuinely have no
/// response payload / no params - NOT "our heuristic failed to find one."
/// Anything else the heuristics can't resolve is a hard build-time error, so
/// a future codex schema change that introduces a new shape breaks the build
/// loudly instead of silently generating wrong code (dropped params, dropped
/// response data). See README.md "Regenerating the schema".
const KNOWN_VOID_RESPONSE_METHODS: &[&str] = &["config/mcpServer/reload"];

fn camel_token_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[A-Z]?[a-z0-9]+|[A-Z]+(?=[A-Z]|$)").expect("camel-token regex is valid")
    })
}

/// Splits a single camelCase/PascalCase word into lowercase tokens, treating
/// runs of consecutive capitals as an acronym token of their own (e.g.
/// `"HTTPServer"` -> `["http", "server"]`). Exact regex ported from
/// `build_combined_schema.py`: `[A-Z]?[a-z0-9]+|[A-Z]+(?=[A-Z]|$)`.
pub fn camel_tokens(word: &str) -> Vec<String> {
    camel_token_regex()
        .find_iter(word)
        .map(|m| {
            m.unwrap_or_else(|e| panic!("camel-token regex failed to match {word:?}: {e}"))
                .as_str()
                .to_lowercase()
        })
        .filter(|t| !t.is_empty())
        .collect()
}

/// `snake_case` function name derived from a method name, e.g.
/// `"item/tool/requestUserInput"` -> `"item_tool_request_user_input"`.
pub fn method_to_snake_fn(method: &str) -> String {
    let mut tokens = Vec::new();
    for segment in method.split('/') {
        tokens.extend(camel_tokens(segment));
    }
    tokens.join("_")
}

/// Finds a `*Response` definition whose name's tokens are a superset of the
/// method's tokens, preferring the shortest match. Bails if two or more
/// candidates tie for shortest - an unresolvable ambiguity, not something to
/// silently guess at (a future codex schema change that introduces a second
/// plausible `*Response` name for some method should break the build, not
/// silently wire that method to whichever candidate happened to sort first).
/// A single strictly-shortest candidate is treated as an unambiguous match
/// even when longer candidates also exist.
pub fn fuzzy_response_match<'a>(
    method: &str,
    all_def_names: impl Iterator<Item = &'a str>,
) -> Result<Option<String>> {
    let method_tokens: BTreeSet<String> = method.split(['/', '_']).flat_map(camel_tokens).collect();

    let mut candidates: Vec<&str> = Vec::new();
    for name in all_def_names {
        let Some(base) = name.strip_suffix("Response") else {
            continue;
        };
        let base_tokens: BTreeSet<String> = camel_tokens(base).into_iter().collect();
        if method_tokens.is_subset(&base_tokens) {
            candidates.push(name);
        }
    }
    candidates.sort_by_key(|c| c.len());

    match candidates.as_slice() {
        [] => Ok(None),
        [only] => Ok(Some(only.to_string())),
        [shortest, rest @ ..] => {
            let tied: Vec<&&str> = rest.iter().filter(|c| c.len() == shortest.len()).collect();
            if !tied.is_empty() {
                bail!(
                    "{method}: ambiguous fuzzy response-type match - multiple *Response types tie \
                     for shortest token-superset match: {:?} (all candidates: {:?}). Add an explicit \
                     RESPONSE_OVERRIDES entry for this method instead of guessing.",
                    std::iter::once(*shortest).chain(tied.into_iter().copied()).collect::<Vec<_>>(),
                    candidates
                );
            }
            Ok(Some(shortest.to_string()))
        }
    }
}

/// Resolves a method's response type name: `RESPONSE_OVERRIDES`, then the
/// `{PascalMethod}Response` naming convention, then a fuzzy token-subset
/// match, then `KNOWN_VOID_RESPONSE_METHODS`. Hard-fails (matching the
/// Python original's `resolve_response`) if none of those resolve it, so a
/// future codex schema change with a genuinely new naming shape breaks the
/// build loudly instead of silently generating wrong code.
pub fn resolve_response(
    method: &str,
    combined_defs: &Map<String, Value>,
) -> Result<Option<String>> {
    let candidate = response_override(method)
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}Response", method_to_pascal(method)));
    if combined_defs.contains_key(&candidate) {
        return Ok(Some(candidate));
    }
    if let Some(fuzzy) = fuzzy_response_match(method, combined_defs.keys().map(String::as_str))? {
        return Ok(Some(fuzzy));
    }
    if KNOWN_VOID_RESPONSE_METHODS.contains(&method) {
        return Ok(None);
    }
    bail!(
        "{method}: could not resolve a response type (checked RESPONSE_OVERRIDES, the {}Response \
         naming convention, and a fuzzy token-subset match) and it is not in \
         KNOWN_VOID_RESPONSE_METHODS. Either add an override, add a fuzzy-matchable response \
         type, or - only if you've confirmed by hand that this method truly returns no payload - \
         add it to KNOWN_VOID_RESPONSE_METHODS.",
        method_to_pascal(method)
    )
}

#[cfg(test)]
#[path = "naming_tests.rs"]
mod tests;
