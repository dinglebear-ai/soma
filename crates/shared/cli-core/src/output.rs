//! Output-format selection.
//!
//! Most CLI subcommands support a human-readable default and a `--json`
//! escape hatch for scripting. [`OutputFormat`] names that choice so
//! commands can branch on it instead of threading a raw `bool` around.

/// The two output shapes a CLI subcommand typically supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

impl OutputFormat {
    /// Map the conventional `--json` boolean flag to a format.
    pub fn from_json_flag(json: bool) -> Self {
        if json {
            Self::Json
        } else {
            Self::Human
        }
    }

    pub fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }

    pub fn is_human(self) -> bool {
        matches!(self, Self::Human)
    }
}

/// Render `human` or `json` depending on `format`, returning owned `String`s
/// so callers can print, log, or compare either branch uniformly.
pub fn render<H, J>(format: OutputFormat, human: H, json: J) -> String
where
    H: FnOnce() -> String,
    J: FnOnce() -> String,
{
    match format {
        OutputFormat::Human => human(),
        OutputFormat::Json => json(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_json_flag_maps_correctly() {
        assert_eq!(OutputFormat::from_json_flag(true), OutputFormat::Json);
        assert_eq!(OutputFormat::from_json_flag(false), OutputFormat::Human);
    }

    #[test]
    fn render_dispatches_to_matching_branch() {
        let out = render(
            OutputFormat::Json,
            || "human".to_string(),
            || "json".to_string(),
        );
        assert_eq!(out, "json");
    }
}
