//! Guest-side helpers for the `soma:provider@1.0.0` component world.
//!
//! Business logic should implement [`ProviderCore`], leaving the tiny
//! `wit-bindgen` export adapter in the final component crate. The same core can
//! then be reused by PyO3 and native-provider adapters.

wit_bindgen::generate!({
    path: "../../../wit/soma-provider",
    world: "provider",
});

/// Reusable business-logic boundary shared by component, PyO3, and native
/// provider adapters.
pub trait ProviderCore {
    fn invoke(input: serde_json::Value) -> Result<serde_json::Value, String>;
}

/// Decode the canonical JSON envelope, invoke a reusable core, and encode its
/// output for the WIT adapter.
pub fn invoke_json<P: ProviderCore>(input: String) -> Result<String, String> {
    let input = serde_json::from_str(&input).map_err(|error| error.to_string())?;
    let output = P::invoke(input)?;
    serde_json::to_string(&output).map_err(|error| error.to_string())
}

/// Perform a capability-mediated HTTP request.
pub fn http(request: &impl serde::Serialize) -> Result<serde_json::Value, String> {
    let request = serde_json::to_string(request).map_err(|error| error.to_string())?;
    let result = soma::provider::host::http(&request)?;
    serde_json::from_str(&result).map_err(|error| error.to_string())
}

/// Resolve a named secret handle declared by the provider.
pub fn secret(name: &str) -> Result<String, String> {
    soma::provider::host::secret(name)
}

/// Read a JSON value from the provider's declared state namespace.
pub fn state_get(key: &str) -> Result<serde_json::Value, String> {
    let result = soma::provider::host::state_get(key)?;
    serde_json::from_str(&result).map_err(|error| error.to_string())
}

/// Write a JSON value into the provider's declared state namespace.
pub fn state_put(key: &str, value: &impl serde::Serialize) -> Result<(), String> {
    let value = serde_json::to_string(value).map_err(|error| error.to_string())?;
    soma::provider::host::state_put(key, &value)
}

/// Emit a bounded structured provider log.
pub fn log(level: &str, message: &str, fields: &impl serde::Serialize) -> Result<(), String> {
    let fields = serde_json::to_string(fields).map_err(|error| error.to_string())?;
    soma::provider::host::log(level, message, &fields)
}

/// Emit a provider metric.
pub fn metric(name: &str, value: f64, attributes: &impl serde::Serialize) -> Result<(), String> {
    let attributes = serde_json::to_string(attributes).map_err(|error| error.to_string())?;
    soma::provider::host::metric(name, value, &attributes)
}

/// Report invocation progress.
pub fn progress(current: u64, total: Option<u64>, message: Option<&str>) -> Result<(), String> {
    soma::provider::host::progress(current, total, message)
}
