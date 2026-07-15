pub fn normalize_user_code(code: &str) -> String {
    let code = strip_code_fences(code.trim()).trim();
    if code.is_empty() {
        return "async () => {}".to_string();
    }
    if let Some(inner) = code.strip_prefix("export default ") {
        return normalize_export_default(inner);
    }
    if let Some(name) = function_declaration_name(code) {
        return format!("async () => {{\n{code}\nreturn {name}();\n}}");
    }
    if is_bare_arrow_expression(code) || is_bare_function_expression(code) {
        return code.trim_end_matches(';').trim().to_string();
    }
    if let Some((before, after)) = code.rsplit_once(';') {
        let trailing = after.trim();
        if !trailing.is_empty() && looks_like_expression(trailing) {
            return format!("async () => {{\n{before};\nreturn ({trailing})\n}}");
        }
    } else if looks_like_expression(code) && !code.trim_start().starts_with("return ") {
        return format!("async () => {{\nreturn ({code})\n}}");
    }
    format!("async () => {{\n{code}\n}}")
}

fn normalize_export_default(inner: &str) -> String {
    let inner = inner.trim().trim_end_matches(';').trim();
    if inner.starts_with("async function") || inner.starts_with("function") {
        return format!("async () => {{\nreturn ({inner})();\n}}");
    }
    if inner.starts_with("class") {
        return format!("async () => {{\nreturn ({inner});\n}}");
    }
    normalize_user_code(inner)
}

fn strip_code_fences(code: &str) -> &str {
    let trimmed = code.trim();
    for lang in ["javascript", "typescript", "tsx", "jsx", "js", "ts", ""] {
        let prefix = if lang.is_empty() {
            "```\n".to_string()
        } else {
            format!("```{lang}\n")
        };
        if trimmed.starts_with(&prefix) && trimmed.ends_with("```") {
            let without_prefix = &trimmed[prefix.len()..trimmed.len() - 3];
            return without_prefix.trim();
        }
    }
    trimmed
}

fn function_declaration_name(code: &str) -> Option<&str> {
    let trimmed = code.trim_start();
    let rest = trimmed
        .strip_prefix("async function ")
        .or_else(|| trimmed.strip_prefix("function "))?;
    let end = rest.find(|ch: char| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))?;
    let name = &rest[..end];
    (!name.is_empty()).then_some(name)
}

fn is_bare_function_expression(code: &str) -> bool {
    let trimmed = code.trim_start();
    trimmed.starts_with("async function") || trimmed.starts_with("function")
}

fn is_bare_arrow_expression(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed.starts_with("async ") && trimmed.contains("=>")
        || trimmed.starts_with('(') && trimmed.contains("=>")
}

fn looks_like_expression(code: &str) -> bool {
    let trimmed = code.trim();
    !(trimmed.starts_with("let ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("var ")
        || trimmed.starts_with("if ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("throw ")
        || trimmed.ends_with('}'))
}
