use serde_json::Value;

pub fn generate_discovery_js(entries: &[Value], blend_weight: f32) -> Result<String, String> {
    let catalog = serde_json::to_string(entries).map_err(|err| err.to_string())?;
    Ok(format!(
        "globalThis.__codemodeDiscovery = {catalog};\nglobalThis.__codemodeBlendWeight = {blend_weight};\n"
    ))
}
