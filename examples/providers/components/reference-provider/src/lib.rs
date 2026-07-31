wit_bindgen::generate!({
    path: "../../../../wit/soma-provider",
    world: "provider",
});

struct ReferenceCore;

impl soma_provider_guest::ProviderCore for ReferenceCore {
    fn invoke(input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "ok": true,
            "echo": input
                .get("arguments")
                .or_else(|| input.get("params"))
                .and_then(|arguments| arguments.get("value"))
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        }))
    }
}

struct ReferenceProvider;

impl Guest for ReferenceProvider {
    fn invoke(input_json: String) -> Result<String, String> {
        soma_provider_guest::invoke_json::<ReferenceCore>(input_json)
    }
}

export!(ReferenceProvider);
