#[tokio::main]
async fn main() -> anyhow::Result<()> {
    synapse::run(std::env::args_os()).await
}

#[cfg(test)]
#[path = "synapse_tests.rs"]
mod tests;
