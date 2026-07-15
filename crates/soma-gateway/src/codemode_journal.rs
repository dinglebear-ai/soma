use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub namespace: String,
    pub action: String,
    pub payload: Value,
}

impl JournalEntry {
    #[must_use]
    pub fn redacted(&self) -> Value {
        crate::security::redact::redact_json_value(&self.payload)
    }
}

#[cfg(test)]
#[path = "codemode_journal_tests.rs"]
mod tests;
