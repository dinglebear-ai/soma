wit_bindgen::generate!({
    path: "wit",
    world: "provider",
});

struct ComponentProvider;

impl Guest for ComponentProvider {
    fn invoke(input_json: String) -> Result<String, String> {
        let input = serde_json::from_str(&input_json).map_err(|error| error.to_string())?;
        let output = crate::core::invoke(input)?;
        serde_json::to_string(&output).map_err(|error| error.to_string())
    }
}

export!(ComponentProvider);
