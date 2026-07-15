use std::time::Duration;

use crate::error::OpenApiError;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const PER_CALL_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) fn base_builder() -> reqwest::ClientBuilder {
    drop(rustls::crypto::ring::default_provider().install_default());
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .https_only(true)
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(PER_CALL_TIMEOUT)
}

pub fn build_dispatch_client() -> Result<reqwest::Client, OpenApiError> {
    base_builder()
        .build()
        .map_err(|_| OpenApiError::ClientBuildFailed)
}

#[cfg(test)]
pub(crate) fn build_loopback_test_client() -> reqwest::Client {
    drop(rustls::crypto::ring::default_provider().install_default());
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .expect("loopback test client")
}
