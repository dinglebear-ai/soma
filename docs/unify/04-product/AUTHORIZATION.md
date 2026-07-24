# Authorization and Data-Sensitivity Model

Soma's existing OAuth and authorization foundation remains authoritative.

## Resource scopes

Suggested authorization resources:

```text
knowledge.source
knowledge.document
observation.record
context.query
graph.entity
graph.evidence
memory.record
job
admin.reindex
admin.retention
```

Actions follow `read`, `create`, `update`, `delete`, `execute`, and `admin` semantics as appropriate.

## Sensitivity

Canonical and derived records carry:

```text
public
internal
sensitive
secret
```

A derived record inherits at least the maximum sensitivity of its evidence unless a reviewed declassification rule applies.

## Retrieval enforcement

Authorization is applied before each retrieval lane returns candidates:

- SQL rows;
- FTS rows;
- Qdrant payload filters;
- graph entities/evidence;
- memory records.

Filtering only after fusion is prohibited because it can leak counts, aliases, scores, or snippets.

## Citations

A user may see a public entity while lacking permission for private evidence. The graph response then reports that evidence exists but is unavailable, without exposing its contents or identifiers beyond policy.

## Source credentials

Adapter credentials are used by runtime clients but:

- never enter manifests;
- never enter source IDs;
- never enter document metadata;
- never enter logs or vector payloads;
- are unavailable through context search.
