//! Local CLI + stdio MCP binary entry point.
//!
//! This is the lightweight plugin/local profile. It does not start the HTTP
//! server; use `example-server serve` for the full API/Web/HTTP MCP profile.

use anyhow::{bail, Result};
use rmcp_template::{cli, runtime};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.as_slice() {
        [f] if matches!(f.as_str(), "--help" | "-h") => {
            eprintln!("{}", cli::usage());
            return Ok(());
        }
        [f] if matches!(f.as_str(), "--version" | "-V" | "version") => {
            println!("example {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        [] | [_, ..] if is_http_server_request(&args) => {
            bail!("HTTP server mode lives in `example-server`; run `example-server serve`")
        }
        _ => {}
    }

    let stdio_mode = matches!(args.as_slice(), [c] if c == "mcp");
    runtime::init_logging(stdio_mode, false);

    if stdio_mode {
        runtime::serve_stdio_mcp().await
    } else {
        runtime::run_cli().await
    }
}

fn is_http_server_request(args: &[String]) -> bool {
    args.is_empty()
        || matches!(args, [c] if c == "serve")
        || matches!(args, [a, b] if a == "serve" && b == "mcp")
}

#[cfg(test)]
#[path = "example_tests.rs"]
mod tests;
