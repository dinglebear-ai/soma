use std::net::IpAddr;

use url::Url;

use crate::{ArtifactTransportPolicy, Result, UpdateDirective, UpdateError};

impl UpdateDirective {
    /// Resolves the artifact reference against a heartbeat/API endpoint.
    pub fn resolve_artifact_url(
        &self,
        endpoint: &Url,
        policy: ArtifactTransportPolicy,
    ) -> Result<Url> {
        if endpoint.cannot_be_a_base() || endpoint.host_str().is_none() {
            return Err(UpdateError::InvalidBaseUrl {
                url: endpoint.to_string(),
                message: "URL must be hierarchical and contain a host".into(),
            });
        }
        let artifact = endpoint.join(self.artifact_url()).map_err(|error| {
            UpdateError::InvalidArtifactUrl {
                url: self.artifact_url().to_owned(),
                message: error.to_string(),
            }
        })?;
        validate_artifact_url(endpoint, &artifact, policy)?;
        Ok(artifact)
    }

    /// Validates one redirect hop or the final response URL against the
    /// authenticated endpoint and transport policy.
    ///
    /// Transport adapters must disable automatic redirects or invoke this for
    /// every redirect target and again for the final response URL.
    pub fn validate_artifact_response_url(
        &self,
        endpoint: &Url,
        response_url: &Url,
        policy: ArtifactTransportPolicy,
    ) -> Result<()> {
        self.resolve_artifact_url(endpoint, policy)?;
        validate_artifact_url(endpoint, response_url, policy)
    }
}

fn validate_artifact_url(
    endpoint: &Url,
    artifact: &Url,
    policy: ArtifactTransportPolicy,
) -> Result<()> {
    if artifact.cannot_be_a_base() || artifact.host_str().is_none() {
        return Err(UpdateError::InvalidArtifactUrl {
            url: artifact.to_string(),
            message: "URL must be hierarchical and contain a host".into(),
        });
    }
    let same_origin = endpoint.scheme() == artifact.scheme()
        && endpoint.host() == artifact.host()
        && endpoint.port_or_known_default() == artifact.port_or_known_default();
    if !same_origin {
        return Err(UpdateError::CrossOriginArtifact {
            base: endpoint.to_string(),
            artifact: artifact.to_string(),
        });
    }
    let secure = artifact.scheme() == "https";
    let loopback_http =
        artifact.scheme() == "http" && is_loopback(artifact.host_str().unwrap_or_default());
    if !(secure || policy == ArtifactTransportPolicy::HttpsOrLoopbackHttp && loopback_http) {
        return Err(UpdateError::InsecureTransport(artifact.to_string()));
    }
    Ok(())
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}
