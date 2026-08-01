use anyhow::{Context, Result, bail};

pub(super) fn read_pyproject_version(content: &str, package: Option<&str>) -> Result<String> {
    let value: toml::Value = toml::from_str(content).context("invalid TOML")?;
    let table = value
        .get("project")
        .and_then(toml::Value::as_table)
        .context("missing [project] table")?;
    if let Some(expected_name) = package {
        let name = table
            .get("name")
            .and_then(toml::Value::as_str)
            .context("missing project.name")?;
        if name != expected_name {
            bail!("expected Python project {expected_name}, found {name}");
        }
    }
    table
        .get("version")
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned)
        .context("missing project.version")
}

fn python_assignment_value<'a>(line: &'a str, variable: &str) -> Option<&'a str> {
    line.trim_start()
        .strip_prefix(variable)?
        .trim_start()
        .strip_prefix('=')
        .map(str::trim_start)
}

pub(super) fn read_python_assignment_version(
    content: &str,
    variable: Option<&str>,
) -> Result<String> {
    let variable = variable.context("python_assignment requires variable")?;
    let value = content
        .lines()
        .find_map(|line| python_assignment_value(line, variable))
        .with_context(|| format!("missing Python assignment {variable}"))?;
    let value = value.split('#').next().unwrap_or(value).trim();
    for quote in ['"', '\''] {
        if let Some(value) = value
            .strip_prefix(quote)
            .and_then(|value| value.strip_suffix(quote))
        {
            return Ok(value.to_owned());
        }
    }
    bail!("Python assignment {variable} must be a quoted string")
}

pub(super) fn replace_pyproject_version(
    content: &str,
    package: Option<&str>,
    next: &str,
) -> Result<String> {
    read_pyproject_version(content, package)?;
    let mut in_project = false;
    let mut replaced = false;
    let mut output = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[project]" {
            in_project = true;
        } else if in_project && trimmed.starts_with('[') {
            in_project = false;
        }

        let mut next_line = line.to_owned();
        if in_project && trimmed.starts_with("version = ") {
            let leading = &line[..line.len() - line.trim_start().len()];
            next_line = format!(r#"{leading}version = "{next}""#);
            replaced = true;
        }
        output.push(next_line);
    }
    if !replaced {
        bail!("missing Python project version");
    }
    Ok(preserve_trailing_newline(content, output.join("\n")))
}

pub(super) fn replace_python_assignment_version(
    content: &str,
    variable: Option<&str>,
    next: &str,
) -> Result<String> {
    let variable = variable.context("python_assignment requires variable")?;
    read_python_assignment_version(content, Some(variable))?;
    let mut replaced = false;
    let output = content
        .lines()
        .map(|line| {
            if !replaced && python_assignment_value(line, variable).is_some() {
                replaced = true;
                let leading = &line[..line.len() - line.trim_start().len()];
                format!(r#"{leading}{variable} = "{next}""#)
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !replaced {
        bail!("missing Python assignment {variable}");
    }
    Ok(preserve_trailing_newline(content, output))
}

fn preserve_trailing_newline(original: &str, mut output: String) -> String {
    if original.ends_with('\n') {
        output.push('\n');
    }
    output
}
