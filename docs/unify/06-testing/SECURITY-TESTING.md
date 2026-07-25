# Security Test Plan

## Threats

- source content prompt injection;
- secret leakage into logs, FTS, vectors, graph, citations, or synthesis;
- path traversal and symlink escape;
- SSRF and crawler scope escape;
- malicious archives/content types;
- oversized or deeply nested records;
- authorization bypass across retrieval lanes;
- tool/provider credential exposure;
- SQL/FTS/query injection;
- graph inference leaking protected relationships;
- stale/dead citation confusion;
- poisoned memory or model-derived graph assertions.

## Required tests

### Ingestion

- path/symlink policy;
- URL scheme/host policy;
- redirect and DNS rebinding controls;
- body/file/record limits;
- malformed parser inputs;
- source credentials absent from IDs/manifests.

### Storage/index

- synthetic secret corpus never appears in ordinary FTS/vector/graph output;
- SQL parameterization;
- bounded FTS syntax;
- Qdrant payload sensitivity filters;
- canonical evidence authorization.

### Retrieval

- mixed-authority result handling;
- untrusted document instructions cannot override query policy;
- unauthorized records excluded before scoring/fusion;
- protected graph edges do not leak aliases/counts;
- synthesis treats retrieved content as data.

### Operations

- internal services bind loopback by default;
- backup permissions;
- migration files and DB ownership;
- health endpoints avoid sensitive data;
- browser/crawler state protected.

## Fuzz targets

- syslog;
- OTLP conversion;
- transcript JSONL;
- Markdown/code/structured parsers;
- manifest and locator parsing;
- bounded JSON;
- URL routing;
- FTS query parser/normalizer.
