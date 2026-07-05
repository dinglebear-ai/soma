use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProviderError {
    pub kind: &'static str,
    pub schema_version: u32,
    pub code: Box<str>,
    pub provider: Box<str>,
    pub action: Option<Box<str>>,
    pub message: Box<str>,
    pub retryable: bool,
    pub remediation: Box<str>,
    #[serde(skip)]
    private_diagnostics: Option<Box<str>>,
}

impl ProviderError {
    pub fn new(
        code: impl Into<String>,
        provider: impl Into<String>,
        action: Option<String>,
        message: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            kind: "provider_error",
            schema_version: 1,
            code: code.into().into_boxed_str(),
            provider: provider.into().into_boxed_str(),
            action: action.map(String::into_boxed_str),
            message: redact_public(&message.into()).into_boxed_str(),
            retryable: false,
            remediation: remediation.into().into_boxed_str(),
            private_diagnostics: None,
        }
    }

    pub fn validation(
        provider: impl Into<String>,
        action: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(
            code,
            provider,
            Some(action.into()),
            message,
            "Change the provider call input and retry.",
        )
    }

    pub fn execution(
        provider: impl Into<String>,
        action: impl Into<String>,
        error: impl std::fmt::Display,
    ) -> Self {
        let diagnostic = error.to_string();
        Self::new(
            "provider_execution_failed",
            provider,
            Some(action.into()),
            redact_public(&diagnostic),
            "Check provider status and retry after the upstream issue is resolved.",
        )
        .with_private_diagnostics(diagnostic)
    }

    pub fn with_retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    pub fn with_private_diagnostics(mut self, diagnostics: impl Into<String>) -> Self {
        self.private_diagnostics = Some(diagnostics.into().into_boxed_str());
        self
    }

    pub fn log_code(&self) -> (&str, Option<&str>, &str) {
        (&self.provider, self.action.as_deref(), &self.code)
    }
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ProviderError {}

pub fn redact_public(input: &str) -> String {
    let mut output = input.to_owned();
    let markers = [
        "authorization:",
        "bearer ",
        "token=",
        "api_key=",
        "apikey=",
        "cookie:",
        "set-cookie:",
        "password=",
        "secret=",
        "stderr:",
        "body:",
    ];
    let lower = output.to_ascii_lowercase();
    if markers.iter().any(|marker| lower.contains(marker)) {
        output = "[redacted provider diagnostic]".to_owned();
    }
    for (key, value) in std::env::vars() {
        if key.ends_with("_TOKEN")
            || key.ends_with("_KEY")
            || key.ends_with("_SECRET")
            || key.contains("PASSWORD")
        {
            output = output.replace(&key, "[redacted env]");
            if value.len() >= 4 {
                output = output.replace(&value, "[redacted env]");
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::redact_public;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn redacts_sensitive_env_values_from_public_diagnostics() {
        let _guard = ENV_LOCK.lock().expect("env lock should not be poisoned");
        std::env::set_var("RTEMPLATE_TEST_SECRET", "super-secret-value");

        let redacted = redact_public("upstream returned super-secret-value");

        std::env::remove_var("RTEMPLATE_TEST_SECRET");
        assert!(!redacted.contains("super-secret-value"));
        assert_eq!(redacted, "upstream returned [redacted env]");
    }
}
