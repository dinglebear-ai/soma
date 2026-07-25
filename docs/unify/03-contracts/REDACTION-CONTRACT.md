# Redaction and Bounded Data Contract

## Defense in depth

Redaction occurs at:

1. ingestion boundary;
2. canonical metadata construction;
3. diagnostics and tracing;
4. semantic projection;
5. vector payload construction;
6. citation/excerpt output;
7. API/MCP/CLI/web presentation where policy requires.

## Secret classes

Default policies recognize at minimum:

- passwords and passphrases;
- access, refresh, bearer, API and session tokens;
- cookies and authorization headers;
- client secrets;
- private keys;
- database credentials;
- credential-bearing URLs;
- secret environment variable names;
- common cloud/service credential shapes.

Product policy supplies additional key names and patterns.

## Boundedness

Policies configure:

- maximum total bytes;
- maximum string bytes;
- maximum depth;
- maximum array/object items;
- maximum properties;
- allowed required paths;
- truncation marker behavior.

A sanitizer returns the sanitized value plus a report of redacted, omitted, and truncated fields.

## Stable hashes

When deduplication needs secret-bearing input, use a keyed or one-way digest appropriate to the threat model. Raw secrets MUST NOT become stable IDs, graph aliases, FTS text, or vector payload.

## Testing

Each crate that handles untrusted content includes:

- known-secret corpus;
- nested and adversarial JSON;
- URL credentials;
- control characters;
- oversized strings;
- Debug/output snapshots;
- property tests proving output limits.
