use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderOutput {
    pub value: Value,
}

impl ProviderOutput {
    pub fn value(value: Value) -> Self {
        Self { value }
    }

    pub fn json(value: Value) -> Self {
        Self::value(value)
    }

    pub fn into_value(self) -> Value {
        self.value
    }
}
