//! Canonical Soma binary entry point.
//!
//! All mode selection (`invocation`), engine construction (`bootstrap`), and
//! CLI/stdio/HTTP composition (`local`/`stdio`/`http`) live in the library
//! crate behind `soma::run`. This file only starts the async runtime and
//! forwards `argv` — the composition root's process entry point stays
//! minimal (plan section 3.1).

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    soma::run(std::env::args().skip(1)).await
}

#[cfg(test)]
#[path = "soma_tests.rs"]
mod tests;
