# End-to-End Scenario Catalog

## E2E-001: Local repository knowledge

1. Add a local Git repository.
2. Discover and publish generation 1.
3. Verify Markdown, Rust, TypeScript, Python, and prose routing.
4. Query an exact symbol through FTS.
5. Query a conceptual architecture question through vectors.
6. Verify citations resolve to exact files/lines.
7. Modify one file and refresh.
8. Verify only affected items/chunks change and generation 2 becomes visible atomically.

## E2E-002: Web documentation crawl

1. Crawl a controlled documentation site with sitemap.
2. Enforce scope/budget.
3. Capture pages and metadata.
4. Refresh after one page changes/removes.
5. Verify generation diff and citations.

## E2E-003: AI session dual projection

1. Ingest Claude, Codex, and Gemini fixtures.
2. Search session content semantically.
3. Search exact tool/MCP calls operationally.
4. Traverse session → project → repository → tool relationships.
5. Verify all projections cite the same typed session records.

## E2E-004: File-tail recovery

1. Tail a log file.
2. Persist and checkpoint lines.
3. Restart Soma.
4. Append, rotate, truncate, and recreate the file.
5. Verify no accepted record loss and bounded duplicate handling.

## E2E-005: Syslog overload

1. Send mixed RFC3164/RFC5424/CEF.
2. Trigger bounded queue pressure.
3. Verify policy, counters, health, and canonical FTS.
4. Restore storage and verify recovery.

## E2E-006: Docker/OTLP/heartbeat correlation

1. Emit deployment/container transition.
2. Emit application error and OTLP record.
3. Collect heartbeat/inventory around the window.
4. Query temporal investigation.
5. Verify timeline and graph path with canonical citations.

## E2E-007: Semantic observation outbox

1. Persist a qualifying incident window with TEI/Qdrant unavailable.
2. Verify canonical write and pending task.
3. Restore services.
4. Verify idempotent projection and cited semantic retrieval.
5. Expire source evidence and verify cleanup.

## E2E-008: Graph rebuild

1. Build graph projection v1.
2. Query known path.
3. Introduce projector v2.
4. Rebuild while v1 remains queryable.
5. Switch to v2.
6. Verify evidence and no dead edges.

## E2E-009: Authorization

Create public, internal, sensitive, and secret records. Verify every SQL, FTS, Qdrant, graph, memory, API, MCP, CLI, and web lane respects the same principal.

## E2E-010: Backup and restore

Create sources, observations, graph, memory and vectors. Back up, destroy state, restore canonical stores, rebuild derived indexes, and compare saved queries.
