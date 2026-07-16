//! OAuth Client ID Metadata Documents (CIMD) support for soma-auth acting
//! as an Authorization Server.
//!
//! Lets an incoming `client_id` be an `https://` URL pointing at a JSON
//! metadata document instead of requiring prior Dynamic Client Registration
//! (RFC 7591). See `document::fetch_and_validate_client_metadata` for the
//! guarded fetch path and `ssrf` for the SSRF preflight guard it composes.
//!
//! A CIMD document's `redirect_uris` are NOT trusted outright — the
//! consumer in `authorize.rs` filters them through the same
//! `is_allowed_redirect_uri` check DCR-registered clients are held to. See
//! that module's `resolve_client_redirect_uris` for why.

pub mod document;
pub mod ssrf;
