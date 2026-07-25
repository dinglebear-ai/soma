# Definition of Done

## Shared crate

```text
[ ] Contract and crate spec satisfied
[ ] Donor behavior fixtures pass
[ ] Typed errors and cancellation complete
[ ] Limits/redaction tested
[ ] No product dependencies or environment discovery
[ ] Default feature build passes
[ ] Key feature matrix passes
[ ] External consumer fixture passes
[ ] Documentation/examples complete
[ ] Package/license contents verified
[ ] Soma consumes crate in a real slice
[ ] Observability hooks exist
[ ] Security review complete
[ ] Publication gate status recorded
```

## Capability slice

```text
[ ] Shared mechanisms extracted
[ ] Soma application use case implemented
[ ] Runtime/storage/provider wiring complete
[ ] Authorization applied
[ ] CLI/API/MCP/Web use the same application operation
[ ] Durable progress/jobs implemented where required
[ ] Canonical and derived state semantics verified
[ ] Donor parity accepted
[ ] Migration/rebuild behavior tested
[ ] E2E acceptance passes
[ ] Health/doctor/operations documented
[ ] Metrics/traces/logs complete
[ ] Non-goals remain absent
```

## V1 product

```text
[ ] All required capabilities in capability-matrix.yaml complete
[ ] Local, web, GitHub, registry, YouTube, Reddit, and session knowledge sources pass
[ ] Required Cortex observation sources pass
[ ] Canonical SQLite/FTS and selective Qdrant projection pass
[ ] Evidence graph and temporal GraphRAG pass
[ ] Memory v1 passes
[ ] North-star Labby OAuth scenario passes
[ ] Backup/restore, upgrade, retention and reindex pass
[ ] Performance/soak and security gates pass
[ ] Donor data migration/cutover rehearsed
[ ] Existing gateway/auth/provider/surfaces remain regression-free
[ ] APM/agent/Incus-worker scope has not leaked into v1
```
