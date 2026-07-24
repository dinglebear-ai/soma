# Shared Crate Publication Rules

## Publication gate

A crate is publish-ready only after all items pass:

```text
[ ] Boundary ADR accepted
[ ] Soma consumes the crate in an end-to-end capability
[ ] Donor behavior fixtures pass
[ ] Independent external consumer fixture compiles
[ ] Public API review complete
[ ] Typed non-exhaustive errors
[ ] Minimal default features
[ ] Secret-safe Debug and diagnostics
[ ] No product environment variables
[ ] No product/path/git-only dependencies
[ ] README and crate-level docs complete
[ ] License included in package
[ ] cargo package --list reviewed
[ ] cargo package succeeds
[ ] cargo publish --dry-run succeeds
[ ] cargo semver-checks succeeds after first release
[ ] MSRV measured and documented
```

## Versioning

- Leaf crates MAY version independently.
- A tightly coupled family MAY use a coordinated release train when schema evolution requires it.
- `soma-primitives` changes require compatibility analysis across all dependent crates.
- Schema changes MUST identify whether they are additive, source-compatible, serialization-breaking, or behavioral.
- Experimental APIs MUST be feature-gated or clearly marked before `1.0`.

## Feature policy

- Default features SHOULD contain only the dependency-light core.
- SQLite, Qdrant, Spider, Chrome, Tree-sitter grammars, TEI, and LLM providers SHOULD be opt-in.
- `all` MAY exist for testing but SHOULD NOT be recommended for production consumers.
- Every meaningful feature combination MUST be checked in CI.

## Naming rule

The package convention is fixed:

- every public package uses the `soma-` namespace;
- exactly one semantic word follows the prefix;
- the Cargo import path is the underscore form of the package name;
- repository directories use one-word leaf names;
- crates.io availability MUST be checked before scaffolding;
- an unavailable name requires an explicit ADR-backed fallback rather than adding another word.

The package names are proposed final names subject only to availability verification.
