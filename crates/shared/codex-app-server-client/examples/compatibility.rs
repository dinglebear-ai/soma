//! Print the vendored schema version, installed Codex version, and method counts.

use codex_app_server_client::CompatibilityReport;

fn main() {
    let report = CompatibilityReport::current();
    println!("schema codex version: {}", report.schema_codex_version);
    println!(
        "installed codex version: {}",
        report
            .installed_codex_version
            .as_deref()
            .unwrap_or("<missing>")
    );
    println!(
        "schema matches installed: {}",
        report.schema_matches_installed()
    );
    println!(
        "surface: {} client requests, {} server requests, {} notifications",
        report.surface.client_requests,
        report.surface.server_requests,
        report.surface.server_notifications,
    );
}
