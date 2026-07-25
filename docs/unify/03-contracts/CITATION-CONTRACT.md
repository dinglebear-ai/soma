# Citation and Provenance Contract

## Rule

Every material fact returned by the context broker MUST be traceable to canonical content or explicitly labeled as inference.

## Canonical reference

A `CanonicalRef` identifies:

```text
store
record type
record ID
optional version
optional content hash
optional event time
optional source locator
```

References MUST remain resolvable for the lifetime promised by the record's retention class.

## Citation

A citation adds:

- authority class;
- safe bounded excerpt;
- relevance explanation;
- retrieval query when useful;
- redaction status.

The excerpt is convenience. The canonical reference and hash establish identity.

## Source locators

Supported locator families include:

- line;
- byte;
- character;
- time;
- DOM;
- JSON Pointer;
- YAML path;
- XPath;
- CSV row;
- AI-session turn;
- canonical database record.

## Citation lifecycle

- A citation to expiring evidence MUST expose its retention horizon when known.
- Before canonical deletion, promoted durable knowledge must receive a new canonical record and citation.
- Derived Qdrant/graph records MUST be cleaned when citations become dead, unless a promoted canonical record replaces them.
- Reindexed projections retain the same canonical references when source identity is unchanged.

## Authority

Authority is explicit and not inferred from vector score.

Preferred source order for normative requirements:

1. version-matched official specification or official product documentation;
2. deployed source/configuration;
3. current source/configuration;
4. verified runtime observations;
5. issues, PRs, reports, sessions;
6. model-derived summaries.

The correct order may vary by question. Runtime behavior proves what happened, while official documentation proves what should happen.

## Evidence chains

A graph path or diagnosis MUST cite each causal step independently. One citation at the end of a multi-step claim is insufficient when intermediate edges rely on different records.

## Privacy

Citations MUST NOT expose:

- access/refresh tokens;
- cookies;
- passwords;
- private keys;
- authorization headers;
- secret environment values;
- unredacted browser state.

The canonical restricted artifact MAY retain sensitive data under separate policy, but ordinary citations use safe excerpts.
