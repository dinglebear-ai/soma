//! Non-executing drop-in provider inspection commands (`list`, `lint`, `status`).
//!
//! Unlike `soma providers validate|inspect|test`, these never build or dispatch
//! through the live application provider catalog — they only parse manifests on disk via
//! `soma_application::providers::filesystem::FileProviderSource::inspect()`. Safe to
//! run before the runtime touches TS/WASM/MCP/OpenAPI handlers.

use std::path::PathBuf;

use anyhow::Result;
use serde_json::{json, Value};
use soma_application::providers::filesystem::{
    FileProviderSource, ProviderDirectoryInspection, ProviderFileInspectionStatus,
};

use crate::ProviderCommand;

pub fn run_providers_command(command: ProviderCommand) -> Result<()> {
    let json_output = matches!(
        command,
        ProviderCommand::List { json: true, .. }
            | ProviderCommand::Lint { json: true, .. }
            | ProviderCommand::Status { json: true, .. }
    );

    let report = inspect_for_command(&command)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_report_json(&report))?
        );
    } else {
        println!("{}", provider_report_text(report.clone()));
    }

    if matches!(command, ProviderCommand::Lint { .. }) && report.providers_invalid > 0 {
        let _ = std::io::Write::flush(&mut std::io::stdout());
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
pub fn build_provider_report_json(command: &ProviderCommand) -> Result<Value> {
    Ok(provider_report_json(&inspect_for_command(command)?))
}

#[cfg(test)]
pub fn build_provider_report_text(command: &ProviderCommand) -> Result<String> {
    Ok(provider_report_text(inspect_for_command(command)?))
}

fn inspect_for_command(command: &ProviderCommand) -> Result<ProviderDirectoryInspection> {
    let dir = match command {
        ProviderCommand::List { dir, .. }
        | ProviderCommand::Lint { dir, .. }
        | ProviderCommand::Status { dir, .. } => dir
            .clone()
            .or_else(|| std::env::var_os("SOMA_PROVIDER_DIR").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("providers")),
        ProviderCommand::Validate | ProviderCommand::Inspect | ProviderCommand::Test { .. } => {
            unreachable!("only non-executing ProviderCommand variants reach the filesystem CLI")
        }
    };

    Ok(FileProviderSource::new(dir).inspect()?)
}

fn provider_report_json(report: &ProviderDirectoryInspection) -> Value {
    json!({
        "provider_dir": report.root.display().to_string(),
        "exists": report.exists,
        "valid": report.providers_invalid == 0,
        "summary": {
            "loaded": report.providers_loaded,
            "disabled": report.providers_disabled,
            "invalid": report.providers_invalid,
            "skipped": report.providers_skipped,
            "files": report.files.len(),
        },
        "files": report.files.iter().map(|file| {
            json!({
                "path": file.path.display().to_string(),
                "file_name": file.file_name,
                "status": status_label(file.status),
                "provider_id": file.provider_id,
                "provider_kind": file.provider_kind,
                "actions": file.actions,
                "error": file.error,
            })
        }).collect::<Vec<_>>(),
    })
}

fn provider_report_text(report: ProviderDirectoryInspection) -> String {
    let mut output = String::new();

    output.push_str(&format!("Provider directory: {}\n", report.root.display()));
    output.push_str(&format!("Exists: {}\n", report.exists));
    output.push_str(&format!(
        "Summary: {} loaded, {} disabled, {} invalid, {} skipped\n",
        report.providers_loaded,
        report.providers_disabled,
        report.providers_invalid,
        report.providers_skipped
    ));

    if report.files.is_empty() {
        output.push_str("Files: none\n");
        return output;
    }

    output.push_str("Files:\n");
    for file in report.files {
        let provider = file.provider_id.as_deref().unwrap_or("-");
        let kind = file.provider_kind.as_deref().unwrap_or("-");
        let actions = if file.actions.is_empty() {
            "-".to_owned()
        } else {
            file.actions.join(", ")
        };

        output.push_str(&format!(
            "  {} [{}] provider={} kind={} actions={}\n",
            file.file_name,
            status_label(file.status),
            provider,
            kind,
            actions
        ));

        if let Some(error) = file.error {
            output.push_str(&format!("    error: {error}\n"));
        }
    }

    output
}

fn status_label(status: ProviderFileInspectionStatus) -> &'static str {
    match status {
        ProviderFileInspectionStatus::Loaded => "loaded",
        ProviderFileInspectionStatus::Disabled => "disabled",
        ProviderFileInspectionStatus::Invalid => "invalid",
        ProviderFileInspectionStatus::Skipped => "skipped",
    }
}

#[cfg(test)]
#[path = "providers_tests.rs"]
mod providers_tests;
