use std::collections::{BTreeSet, HashSet};

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GeneratedToolTypes {
    pub(crate) signature: String,
    pub(crate) dts: String,
}

pub(crate) fn generate_tool_types(
    namespace: &str,
    tool: &str,
    description: &str,
    input_schema: Option<&Value>,
    output_schema: Option<&Value>,
) -> GeneratedToolTypes {
    let base = format!("{}{}", pascal(namespace), pascal(tool));
    let input_name = format!("{base}Input");
    let output_name = format!("{base}Output");
    let namespace_method = namespace_segment(namespace);
    let tool_method = tool_name_to_snake(tool);
    let input_type = json_schema_to_type(input_schema);
    let output_type = json_schema_to_type(output_schema);
    let signature = format!(
        "codemode.{namespace_method}.{tool_method}(params: {input_name}): Promise<{output_name}>"
    );

    let mut dts = String::new();
    dts.push_str(&format!("type {input_name} = {input_type};\n"));
    dts.push_str(&format!("type {output_name} = {output_type};\n"));
    dts.push_str(&format!(
        "interface Codemode{}Tools {{\n",
        pascal(namespace)
    ));
    if !description.trim().is_empty() {
        dts.push_str("  /** ");
        dts.push_str(&description.replace("*/", "* /"));
        dts.push_str(" */\n");
    }
    dts.push_str(&format!(
        "  {tool_method}(params: {input_name}): Promise<{output_name}>;\n"
    ));
    dts.push_str("}\n");
    dts.push_str("interface CodemodeTools {\n");
    dts.push_str(&format!(
        "  {namespace_method}: Codemode{}Tools;\n",
        pascal(namespace)
    ));
    dts.push_str("}\n");
    dts.push_str("declare var codemode: CodemodeTools;\n");
    dts.push_str(&format!(
        "declare function callTool(id: {}, params: {input_name}): Promise<{output_name}>;\n",
        serde_json::to_string(&crate::types::namespaced_tool_id(namespace, tool))
            .unwrap_or_else(|_| "\"\"".to_string())
    ));

    GeneratedToolTypes { signature, dts }
}

pub(crate) fn json_schema_to_type(schema: Option<&Value>) -> String {
    let Some(schema) = schema else {
        return "unknown".to_string();
    };
    let mut seen = HashSet::new();
    schema_to_type(schema, schema, 0, &mut seen)
}

pub(crate) fn tool_name_to_snake(name: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = false;
    for ch in name.chars() {
        if ch == '-' || ch == '.' || ch.is_whitespace() {
            if !prev_sep {
                out.push('_');
            }
            prev_sep = true;
        } else if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_sep = false;
        } else {
            prev_sep = false;
        }
    }
    let mut out = out.trim_matches('_').to_string();
    if out.is_empty() {
        out = "_".to_string();
    }
    if out.starts_with(|ch: char| ch.is_ascii_digit()) {
        out.insert(0, '_');
    }
    if is_reserved_js_word(&out) {
        out.push('_');
    }
    out
}

pub(crate) fn namespace_segment(name: &str) -> String {
    let mut segment = tool_name_to_snake(name);
    if matches!(
        segment.as_str(),
        "search" | "describe" | "step" | "batch" | "run" | "tools"
    ) {
        segment.push('_');
    }
    segment
}

fn schema_to_type(
    schema: &Value,
    root: &Value,
    depth: usize,
    seen: &mut HashSet<String>,
) -> String {
    if depth > 20 {
        return "unknown".to_string();
    }
    let Some(object) = schema.as_object() else {
        return "unknown".to_string();
    };
    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        if !seen.insert(reference.to_string()) {
            return "unknown".to_string();
        }
        let resolved = reference
            .strip_prefix('#')
            .and_then(|pointer| root.pointer(pointer))
            .map(|schema| schema_to_type(schema, root, depth + 1, seen))
            .unwrap_or_else(|| "unknown".to_string());
        seen.remove(reference);
        return resolved;
    }
    if let Some(value) = object.get("const") {
        return literal_type(value);
    }
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        return join_types(values.iter().map(literal_type), " | ");
    }
    if let Some(values) = object.get("anyOf").and_then(Value::as_array) {
        return join_types(
            values
                .iter()
                .map(|value| schema_to_type(value, root, depth + 1, seen)),
            " | ",
        );
    }
    if let Some(values) = object.get("oneOf").and_then(Value::as_array) {
        return join_types(
            values
                .iter()
                .map(|value| schema_to_type(value, root, depth + 1, seen)),
            " | ",
        );
    }
    if let Some(values) = object.get("allOf").and_then(Value::as_array) {
        return join_types(
            values
                .iter()
                .map(|value| schema_to_type(value, root, depth + 1, seen)),
            " & ",
        );
    }
    match object.get("type") {
        Some(Value::Array(types)) => join_types(
            types
                .iter()
                .filter_map(Value::as_str)
                .map(|kind| schema_type_to_type(kind, schema, root, depth, seen)),
            " | ",
        ),
        Some(Value::String(kind)) => schema_type_to_type(kind, schema, root, depth, seen),
        _ if object.contains_key("properties") => object_type(schema, root, depth, seen),
        _ if object.contains_key("items") => array_type(schema, root, depth, seen),
        _ => "unknown".to_string(),
    }
}

fn schema_type_to_type(
    kind: &str,
    schema: &Value,
    root: &Value,
    depth: usize,
    seen: &mut HashSet<String>,
) -> String {
    match kind {
        "object" => object_type(schema, root, depth, seen),
        "array" => array_type(schema, root, depth, seen),
        "string" if schema.get("format").and_then(Value::as_str) == Some("binary") => {
            "Uint8Array | ArrayBuffer".to_string()
        }
        "string" => "string".to_string(),
        "integer" | "number" => "number".to_string(),
        "boolean" => "boolean".to_string(),
        "null" => "null".to_string(),
        _ => "unknown".to_string(),
    }
}

fn object_type(schema: &Value, root: &Value, depth: usize, seen: &mut HashSet<String>) -> String {
    let Some(object) = schema.as_object() else {
        return "Record<string, unknown>".to_string();
    };
    let required = object
        .get("required")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let Some(properties) = object.get("properties").and_then(Value::as_object) else {
        return "Record<string, unknown>".to_string();
    };
    if properties.is_empty() {
        return "Record<string, unknown>".to_string();
    }
    let mut fields = Vec::new();
    for (key, property) in properties {
        let optional = if required.contains(key.as_str()) {
            ""
        } else {
            "?"
        };
        fields.push(format!(
            "{}{}: {}",
            quote_property(key),
            optional,
            schema_to_type(property, root, depth + 1, seen)
        ));
    }
    format!("{{ {} }}", fields.join("; "))
}

fn array_type(schema: &Value, root: &Value, depth: usize, seen: &mut HashSet<String>) -> String {
    schema
        .get("items")
        .map(|items| format!("Array<{}>", schema_to_type(items, root, depth + 1, seen)))
        .unwrap_or_else(|| "unknown[]".to_string())
}

fn literal_type(value: &Value) -> String {
    match value {
        Value::String(value) => serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into()),
        Value::Number(_) | Value::Bool(_) | Value::Null => value.to_string(),
        _ => "unknown".to_string(),
    }
}

fn join_types(values: impl Iterator<Item = String>, sep: &str) -> String {
    let mut values = values.filter(|value| !value.is_empty()).collect::<Vec<_>>();
    values.sort();
    values.dedup();
    if values.is_empty() {
        "unknown".to_string()
    } else {
        values.join(sep)
    }
}

fn quote_property(key: &str) -> String {
    if is_js_identifier(key) && !is_reserved_js_word(key) {
        key.to_string()
    } else {
        serde_json::to_string(key).unwrap_or_else(|_| "\"\"".into())
    }
}

fn is_js_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

fn is_reserved_js_word(value: &str) -> bool {
    matches!(
        value,
        "await"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "delete"
            | "do"
            | "else"
            | "export"
            | "extends"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "let"
            | "new"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
    )
}

fn pascal(value: &str) -> String {
    let mut out = String::new();
    for part in value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
    {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        "Tool".to_string()
    } else if out.starts_with(|ch: char| ch.is_ascii_digit()) {
        format!("Tool{out}")
    } else {
        out
    }
}
