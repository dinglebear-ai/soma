// Render per-item feature-requirement badges when rustdoc runs on nightly with
// `--cfg docsrs` (docs.rs posture; locally via `cargo xtask doc --docsrs-cfg`).
// Inert under the stable CI doc gate: stable rustdoc never sets `docsrs`.
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
//! Concrete outbound transport for a remote Soma HTTP server.
//!
//! `soma-client` owns HTTP request construction, remote response decoding, and
//! transport-level retries/timeouts for talking to a deployed `soma serve`
//! REST API. It does not decide *when* a request should go upstream (that is
//! application policy), and it has no CLI or provider-registry logic of its
//! own — see plan section 3.19. It does resolve REST method/path per action
//! from the provider catalog (`resolve_remote_rest_call`) and validates the
//! action path segment before building a request; that routing/validation is
//! a transport-shape concern, not business logic, and stays here rather than
//! in `soma-service`.

mod client;

pub use client::SomaClient;
