# Changelog

All notable changes to `soma-auth` are documented here.

## [0.5.0] - 2026-08-04

### Added

- Public `AuthProfile` for application-owned product identity.
- Typed `AuthConfigBuilder::build` path that performs no environment reads.
- `EnvAuthConfigLoader` for optional env-style overlays.
- Consumer-supplied upstream OAuth client name and callback path.
- crates.io, docs.rs, examples, package smoke, and release metadata.

### Changed

- Generic defaults now use `APP`, `auth_session`, `app:read`, `app:admin`, `.auth`, and upstream client name `app`.
- Soma-specific defaults moved to the Soma integration composition root.
- Package version advanced to 0.5.0 for the public configuration contract change.

### Security

- Enforced RFC 7636 PKCE challenge and verifier syntax before one-shot code redemption.
- Hardened redirect URI validation against fragments, userinfo, dangerous schemes, and non-loopback plain HTTP while retaining native-client compatibility.
- Required strong one-time native polling state and rejected active state collisions.
- Rotated public-client refresh tokens, retained spent-token family history, and revoked active families on replay.
- Consumed provider-error state exactly once without reflecting provider diagnostics, including terminal native-poll errors instead of silent timeouts.
- Added no-store headers to OAuth redirects carrying state or authorization codes.
- Sanitized client-visible auth errors while preserving detailed server-side logs.
- Bounded identity-provider success and error response bodies.
- Aligned authorization-server metadata with mounted registration, native, token-auth, signing, and revocation capabilities.
- Validated public URL, cookie name, callback paths, and other externally visible auth configuration.

### Removed

- Hardcoded Soma and Labby runtime identity from the reusable auth engine.
