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

### Removed

- Hardcoded Soma and Labby runtime identity from the reusable auth engine.
