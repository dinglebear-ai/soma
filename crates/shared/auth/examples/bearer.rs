use soma_auth::config::AuthConfigBuilder;

fn main() -> Result<(), soma_auth::error::AuthError> {
    let config = AuthConfigBuilder::new()
        .bootstrap_secret("replace-with-a-secret-manager-value")
        .build()?;

    println!(
        "mode={:?} database={}",
        config.mode,
        config.sqlite_path.display()
    );
    Ok(())
}
