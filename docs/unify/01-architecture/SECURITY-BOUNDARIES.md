# Security Boundaries

## Trust classes

1. trusted in-process Rust;
2. configured external clients;
3. untrusted acquired content;
4. untrusted observation payloads;
5. model-derived claims;
6. caller-provided query text.

## Required controls

- acquired URLs pass SSRF and scheme policy;
- local paths pass allowed-root and symlink policy;
- observation records are bounded before queue admission;
- secrets are redacted before logs, diagnostics, vectors, graph excerpts, and synthesis;
- claimed host/service identity remains distinct from transport-observed identity;
- every query is scoped by the caller's existing Soma authorization context;
- Qdrant payload filters cannot expand caller visibility;
- model-derived relationships carry lower authority and explicit evidence;
- raw browser/session credentials never enter shared contracts.

## Prompt injection

Retrieved content is evidence, not instruction. Context bundles MUST mark source content boundaries and MUST NOT grant capabilities based on retrieved text.

## Destructive actions

V1 context operations are read-oriented except source administration, reindex, prune, retention, and rebuild. Existing Soma destructive-action gates apply.
