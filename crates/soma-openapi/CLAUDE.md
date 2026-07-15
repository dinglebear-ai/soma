# soma-openapi

This crate is a standalone Soma port of Lab's OpenAPI support. It must depend
only on external crates and must not depend on any `soma-*`, `labby-*`, or
`rmcp-openapi` crate.

Keep the parser hand-rolled and behavior-compatible with the live Lab surface:
configured specs load into an `OpenApiRegistry`, operation IDs are allowlisted,
registry load is degraded per spec, and dispatch goes through
`dispatch_openapi_call`.

HTTP dispatch is intentionally hardened. Keep redirects disabled, use rustls,
cap spec and response bodies, validate every resolved address, pin the selected
peer, recheck `remote_addr()`, and reject private, loopback, link-local, CGNAT,
ULA, IPv4 Class E, and IPv6 multicast destinations. The Class E and IPv6
multicast rejection is a deliberate Soma strengthening beyond the Lab source.

Tests live in sibling `*_tests.rs` files. Do not add inline `mod tests`, `mod.rs`,
or any Rust source/test file over 500 physical lines.
