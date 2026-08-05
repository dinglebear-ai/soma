//! Callback error normalization and RFC 9207 authorization-error redirects.

use axum::response::{IntoResponse, Redirect, Response};

use crate::error::AuthError;
use crate::types::{AuthorizationRequestRow, CallbackQuery};
use crate::util::apply_cache_control_no_store;

pub(super) fn provider_callback_error(query: &CallbackQuery) -> Option<&'static str> {
    match query.error.as_deref() {
        Some("access_denied") => Some("access_denied"),
        Some(_) => Some("server_error"),
        None if query
            .code
            .as_deref()
            .is_none_or(|code| code.trim().is_empty()) =>
        {
            Some("server_error")
        }
        None => None,
    }
}

pub(super) fn authorization_error_redirect(
    request: &AuthorizationRequestRow,
    issuer: &str,
    error: &str,
) -> Result<Response, AuthError> {
    let mut redirect_target = url::Url::parse(&request.redirect_uri).map_err(|parse_error| {
        AuthError::Config(format!(
            "failed to parse registered redirect_uri while returning an OAuth error: {parse_error}"
        ))
    })?;
    redirect_target
        .query_pairs_mut()
        .append_pair("error", error)
        .append_pair("state", &request.client_state)
        .append_pair("iss", issuer);
    Ok(apply_cache_control_no_store(
        Redirect::to(redirect_target.as_str()).into_response(),
    ))
}
