use crate::ToolError;

pub fn validate_ref(value: &str) -> Result<(), ToolError> {
    if value.is_empty()
        || value.starts_with('-')
        || value.contains("..")
        || value.contains('\\')
        || value.chars().any(char::is_whitespace)
    {
        return Err(ToolError::InvalidParam {
            message: "unsafe git ref".to_string(),
            param: "ref".to_string(),
        });
    }
    Ok(())
}

pub fn safe_git_env() -> Vec<(&'static str, &'static str)> {
    vec![
        ("GIT_TERMINAL_PROMPT", "0"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_PROTOCOL_FROM_USER", "0"),
    ]
}
