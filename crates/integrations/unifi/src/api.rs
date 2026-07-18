//! Path/URL builders for the two UniFi controller APIs this crate speaks.

pub mod internal;
pub mod official;
pub mod path;

/// Which controller API a [`crate::capabilities::Capability`] is served from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiSourceFamily {
    /// UniFi's documented `/proxy/network/integration` REST API.
    Official,
    /// The controller's own internal (undocumented, but stable) web-UI API.
    Internal,
    /// Resolves to [`Official`](Self::Official) or [`Internal`](Self::Internal)
    /// at call time — see [`crate::actions::hybrid`].
    Hybrid,
}
