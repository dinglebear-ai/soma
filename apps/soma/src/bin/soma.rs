//! Local CLI + stdio MCP binary entry point.
//!
//! The canonical Soma binary. It can run the HTTP server, stdio MCP transport,
//! or CLI adapter depending on the explicit subcommand.

use anyhow::Result;
use soma::{cli, runtime};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.as_slice() {
        [f] if matches!(f.as_str(), "--help" | "-h") => {
            eprintln!("{}", cli::usage());
            return Ok(());
        }
        [f] if matches!(f.as_str(), "--version" | "-V" | "version") => {
            println!("soma {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }

    let stdio_mode = matches!(args.as_slice(), [c] if c == "mcp");
    let serve_mode = is_http_server_request(&args);
    // Load ~/.soma/.env (or SOMA_HOME/.env) for local CLI/plugin runs before
    // any command loads typed config. Explicit process env still wins.
    soma::config::load_dotenv();
    runtime::init_logging(stdio_mode, serve_mode);

    if serve_mode {
        #[cfg(feature = "mcp-http")]
        {
            runtime::serve_http_mcp().await
        }
        #[cfg(not(feature = "mcp-http"))]
        {
            anyhow::bail!("`soma serve` requires the `mcp-http` or `server` feature")
        }
    } else if stdio_mode {
        runtime::serve_stdio_mcp().await
    } else {
        runtime::run_cli().await
    }
}

fn is_http_server_request(args: &[String]) -> bool {
    matches!(args, [c] if c == "serve") || matches!(args, [a, b] if a == "serve" && b == "mcp")
}

#[cfg(test)]
#[path = "soma_tests.rs"]
mod tests;
