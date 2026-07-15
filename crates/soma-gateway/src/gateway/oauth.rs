use thiserror::Error;

use crate::config::{GatewayUpstreamOauthMode, GatewayUpstreamOauthRegistration, UpstreamConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayOAuthSurface {
    AdminOAuthOperation,
    CodeModeAdmin,
    CodeModeShared,
    ProtectedPublicRoute,
    RelayToolCall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityMatrixRow {
    pub surface: GatewayOAuthSurface,
    pub authenticated_caller: &'static str,
    pub upstream_credential_subject: &'static str,
    pub relay_cache_subject: &'static str,
    pub protected_public_route_caller_subject: &'static str,
    pub source_of_truth: &'static str,
    pub caller_supplied_subject_accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewaySubject {
    pub subject: String,
    pub caller_supplied: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GatewayOAuthError {
    #[error("upstream oauth config requires oauth")]
    MissingOauthConfig,
    #[error("upstream oauth config requires url")]
    MissingUrl,
    #[error("caller supplied subject is accepted only for admin OAuth operations")]
    CallerSuppliedSubjectDenied,
}

pub fn to_soma_auth_upstream_config(
    upstream: &UpstreamConfig,
) -> Result<soma_auth::upstream::config::UpstreamConfig, GatewayOAuthError> {
    let oauth = upstream
        .oauth
        .as_ref()
        .ok_or(GatewayOAuthError::MissingOauthConfig)?;
    let url = upstream.url.clone().ok_or(GatewayOAuthError::MissingUrl)?;
    Ok(soma_auth::upstream::config::UpstreamConfig {
        name: upstream.name.clone(),
        url: Some(url),
        oauth: Some(soma_auth::upstream::config::UpstreamOauthConfig {
            mode: match oauth.mode {
                GatewayUpstreamOauthMode::AuthorizationCodePkce => {
                    soma_auth::upstream::config::UpstreamOauthMode::AuthorizationCodePkce
                }
            },
            registration: match &oauth.registration {
                GatewayUpstreamOauthRegistration::ClientMetadataDocument { url } => {
                    soma_auth::upstream::config::UpstreamOauthRegistration::ClientMetadataDocument {
                        url: url.clone(),
                    }
                }
                GatewayUpstreamOauthRegistration::Preregistered {
                    client_id,
                    client_secret_env,
                } => soma_auth::upstream::config::UpstreamOauthRegistration::Preregistered {
                    client_id: client_id.clone(),
                    client_secret_env: client_secret_env.clone(),
                },
                GatewayUpstreamOauthRegistration::Dynamic => {
                    soma_auth::upstream::config::UpstreamOauthRegistration::Dynamic
                }
            },
            scopes: oauth.scopes.clone(),
            prefer_client_metadata_document: oauth.prefer_client_metadata_document,
        }),
    })
}

pub fn identity_matrix() -> Vec<IdentityMatrixRow> {
    vec![
        IdentityMatrixRow {
            surface: GatewayOAuthSurface::AdminOAuthOperation,
            authenticated_caller: "admin principal",
            upstream_credential_subject: "caller selected subject",
            relay_cache_subject: "caller selected subject",
            protected_public_route_caller_subject: "not applicable",
            source_of_truth: "admin OAuth params",
            caller_supplied_subject_accepted: true,
        },
        IdentityMatrixRow {
            surface: GatewayOAuthSurface::CodeModeAdmin,
            authenticated_caller: "admin principal",
            upstream_credential_subject: "shared gateway subject",
            relay_cache_subject: "shared gateway subject",
            protected_public_route_caller_subject: "not applicable",
            source_of_truth: "gateway config",
            caller_supplied_subject_accepted: false,
        },
        IdentityMatrixRow {
            surface: GatewayOAuthSurface::CodeModeShared,
            authenticated_caller: "shared gateway principal",
            upstream_credential_subject: "shared gateway subject",
            relay_cache_subject: "shared gateway subject",
            protected_public_route_caller_subject: "not applicable",
            source_of_truth: "gateway config",
            caller_supplied_subject_accepted: false,
        },
        IdentityMatrixRow {
            surface: GatewayOAuthSurface::ProtectedPublicRoute,
            authenticated_caller: "public route caller",
            upstream_credential_subject: "shared gateway subject",
            relay_cache_subject: "shared gateway subject",
            protected_public_route_caller_subject: "stripped before upstream",
            source_of_truth: "protected route config",
            caller_supplied_subject_accepted: false,
        },
        IdentityMatrixRow {
            surface: GatewayOAuthSurface::RelayToolCall,
            authenticated_caller: "downstream MCP session",
            upstream_credential_subject: "resolved gateway subject",
            relay_cache_subject: "resolved gateway subject",
            protected_public_route_caller_subject: "not applicable",
            source_of_truth: "gateway-authenticated principal",
            caller_supplied_subject_accepted: false,
        },
    ]
}

pub fn resolve_subject(
    surface: GatewayOAuthSurface,
    configured_subject: &str,
    caller_supplied_subject: Option<&str>,
) -> Result<GatewaySubject, GatewayOAuthError> {
    if let Some(subject) = caller_supplied_subject {
        if surface == GatewayOAuthSurface::AdminOAuthOperation {
            return Ok(GatewaySubject {
                subject: subject.to_owned(),
                caller_supplied: true,
            });
        }
        return Err(GatewayOAuthError::CallerSuppliedSubjectDenied);
    }
    Ok(GatewaySubject {
        subject: configured_subject.to_owned(),
        caller_supplied: false,
    })
}

pub fn strip_public_authorization_header<'a>(
    headers: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<(&'a str, &'a str)> {
    headers
        .into_iter()
        .filter(|(name, _)| !name.eq_ignore_ascii_case("authorization"))
        .collect()
}

#[cfg(test)]
#[path = "oauth_tests.rs"]
mod tests;
