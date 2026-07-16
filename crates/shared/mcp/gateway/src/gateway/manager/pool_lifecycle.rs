use crate::config::GatewayConfig;
use crate::upstream::pool::{PoolOptions, UpstreamPool};
use crate::upstream::UpstreamError;

pub fn build_pool_from_config(config: &GatewayConfig) -> Result<UpstreamPool, UpstreamError> {
    let pool = UpstreamPool::new(PoolOptions::default());
    for upstream in &config.upstream {
        pool.register_config(upstream.clone())?;
    }
    Ok(pool)
}

#[cfg(test)]
#[path = "pool_lifecycle_tests.rs"]
mod tests;
