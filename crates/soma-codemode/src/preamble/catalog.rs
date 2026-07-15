use std::collections::BTreeMap;

use crate::types::ToolDescriptor;

use super::names::{namespace_segment, tool_name_to_snake};

pub fn generate_js_proxy_from_catalog(entries: &[ToolDescriptor]) -> Result<String, String> {
    let mut namespaces: BTreeMap<String, Vec<&ToolDescriptor>> = BTreeMap::new();
    for entry in entries {
        namespaces
            .entry(entry.namespace.clone())
            .or_default()
            .push(entry);
    }
    let mut out = String::from("globalThis.codemode = globalThis.codemode || {};\n");
    out.push_str("var codemode = globalThis.codemode;\n");
    for (namespace, tools) in namespaces {
        let ns = namespace_segment(&namespace);
        out.push_str(&format!("codemode.{ns} = codemode.{ns} || {{}};\n"));
        for tool in tools {
            let name = tool_name_to_snake(&tool.name);
            let id = serde_json::to_string(&tool.id).map_err(|err| err.to_string())?;
            out.push_str(&format!(
                "codemode.{ns}.{name} = (params = {{}}) => callTool({id}, params);\n"
            ));
        }
    }
    Ok(out)
}
