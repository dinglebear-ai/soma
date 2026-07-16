use serde::Serialize;

#[derive(Debug, Clone)]
pub enum ToolError {
    UnknownAction {
        message: String,
        valid: Vec<String>,
        hint: Option<String>,
    },
    MissingParam {
        message: String,
        param: String,
    },
    InvalidParam {
        message: String,
        param: String,
    },
    UnknownInstance {
        message: String,
        valid: Vec<String>,
    },
    AmbiguousTool {
        message: String,
        valid: Vec<String>,
    },
    ConfirmationRequired {
        message: String,
    },
    Conflict {
        message: String,
        existing_id: String,
    },
    Forbidden {
        message: String,
        required_scopes: Vec<String>,
    },
    Sdk {
        sdk_kind: String,
        message: String,
    },
}

impl Serialize for ToolError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value = match self {
            Self::UnknownAction {
                message,
                valid,
                hint,
            } => serde_json::json!({
                "kind": "unknown_action",
                "message": message,
                "valid": valid,
                "hint": hint,
            }),
            Self::MissingParam { message, param } => {
                serde_json::json!({"kind": "missing_param", "message": message, "param": param})
            }
            Self::InvalidParam { message, param } => {
                serde_json::json!({"kind": "invalid_param", "message": message, "param": param})
            }
            Self::UnknownInstance { message, valid } => {
                serde_json::json!({"kind": "unknown_instance", "message": message, "valid": valid})
            }
            Self::AmbiguousTool { message, valid } => {
                serde_json::json!({"kind": "ambiguous_tool", "message": message, "valid": valid})
            }
            Self::ConfirmationRequired { message } => {
                serde_json::json!({"kind": "confirmation_required", "message": message})
            }
            Self::Conflict {
                message,
                existing_id,
            } => serde_json::json!({
                "kind": "conflict",
                "message": message,
                "existing_id": existing_id,
            }),
            Self::Forbidden {
                message,
                required_scopes,
            } => serde_json::json!({
                "kind": "forbidden",
                "message": message,
                "required_scopes": required_scopes,
            }),
            Self::Sdk { sdk_kind, message } => {
                serde_json::json!({"kind": sdk_kind, "message": message})
            }
        };
        value.serialize(serializer)
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match serde_json::to_string(self) {
            Ok(value) => f.write_str(&value),
            Err(_) => write!(f, "{self:?}"),
        }
    }
}

impl std::error::Error for ToolError {}

impl ToolError {
    #[must_use]
    pub fn kind(&self) -> &str {
        match self {
            Self::UnknownAction { .. } => "unknown_action",
            Self::MissingParam { .. } => "missing_param",
            Self::InvalidParam { .. } => "invalid_param",
            Self::UnknownInstance { .. } => "unknown_instance",
            Self::AmbiguousTool { .. } => "ambiguous_tool",
            Self::ConfirmationRequired { .. } => "confirmation_required",
            Self::Conflict { .. } => "conflict",
            Self::Forbidden { .. } => "forbidden",
            Self::Sdk { sdk_kind, .. } => sdk_kind.as_str(),
        }
    }

    #[must_use]
    pub fn user_message(&self) -> &str {
        match self {
            Self::UnknownAction { message, .. }
            | Self::MissingParam { message, .. }
            | Self::InvalidParam { message, .. }
            | Self::UnknownInstance { message, .. }
            | Self::AmbiguousTool { message, .. }
            | Self::ConfirmationRequired { message }
            | Self::Conflict { message, .. }
            | Self::Forbidden { message, .. }
            | Self::Sdk { message, .. } => message.as_str(),
        }
    }

    #[must_use]
    pub fn internal_message(message: impl Into<String>) -> Self {
        Self::Sdk {
            sdk_kind: "internal_error".to_string(),
            message: message.into(),
        }
    }
}
