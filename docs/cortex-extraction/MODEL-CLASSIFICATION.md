---
title: "Cortex Model Classification"
created: 2026-08-18
updated: 2026-08-18
doc_type: "report"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-08-18"
---

# Cortex model classification

This inventory classifies every public type declared in the Cortex donor `src/app/models/*.rs` surface at commit `7edf23fadb94650c2d2a2f9c80111fb44319eea8`. The classification is about ownership, not about where the donor happens to define the type today.

## Decision rules

- **semantic contract**: meaning survives replacement of SQLite, HTTP/MCP/CLI, and process/runtime implementations; these are eligible for `cortex-domain`.
- **storage/query projection**: persistence statistics, database-maintenance state, or query-shaped rows whose ownership belongs with the storage adapter/application query layer.
- **transport DTO/policy**: request/response envelopes, pagination/filter input, surface-specific policy, and response-navigation metadata.
- **runtime/collector state**: OS/process/collector implementation state that belongs to the runtime capability producing it.

Current totals: **255 public types**: 65 semantic, 165 transport, 23 storage/query, 2 runtime. Wave 1 extracts the stable semantic subset that is already useful without later storage/transport crates; semantic types still embedded in response-only aggregates remain assigned to the domain boundary but can move at cutover without changing this ownership decision.

## Complete type inventory

## `ai_hook_incidents.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `AiHookIncidentRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HookSignalCounts` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `HookIncident` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AiHookIncidentResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiHookInvestigateRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HookIncidentEvidence` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `HookIncidentSummary` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AiHookInvestigateResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `ai_incidents.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `AiIncidentRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiIncidentResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AbuseIncident` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AiInvestigateRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `IncidentEvidence` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AiInvestigateResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiAssessRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiAssessEvidenceSummary` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AiAssessResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AbuseAssessRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AbuseAssessResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiCorrelateRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiCorrelationAnchor` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AiCorrelateResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `TopicCorrelateRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ResolvedTopicEntity` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `TopicExpansionEntity` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `TopicTimelineEntry` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `TopicCorrelateResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `CorrelatedLogRow` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `GraphSessionCorrelation` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |

## `ai_inventory.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `UsageBlocksRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `UsageBlock` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `UsageBlocksResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ProjectContextRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ProjectContextResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ListAiToolsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiToolEntry` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `ListAiToolsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ListAiProjectsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiProjectEntry` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `ListAiProjectsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `ai_mcp_incidents.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `AiMcpIncidentRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `McpSignalCounts` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `McpIncident` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AiMcpIncidentResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiMcpInvestigateRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `McpIncidentEvidence` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `McpIncidentSummary` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AiMcpInvestigateResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `ai_sessions.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `ListSessionsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ListSessionsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiSessionEntry` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `SearchSessionsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `SearchedSessionEntry` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `SearchSessionsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AbuseSearchRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AbuseMatch` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AbuseSearchResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `ai_skill_incidents.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `AiSkillIncidentRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `SkillSignalCounts` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `SkillIncident` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AiSkillIncidentResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiSkillInvestigateRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `SkillIncidentEvidence` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `SkillIncidentSummary` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AiSkillInvestigateResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `context.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `ContextRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ContextResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GetLogRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GetLogResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `FeedLogsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `FeedLogsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `LogEntryWithRaw` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `IngestRateRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `IngestRateResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `IngestRateBuckets` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `IngestRatePerHost` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `SilentHostsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `SilentHostsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `SilentHostEntry` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `ClockSkewRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ClockSkewResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ClockSkewEntry` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `AnomaliesRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AnomaliesResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AnomalyEntry` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `CompareRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `CompareResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `RangeSummary` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |

## `core.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `RequestActor` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AiCorrelateLimitPolicy` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiLimitPolicy` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `DbMaintenanceStatus` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `DbCheckpointResult` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `DbVacuumResult` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `DbIntegrityResult` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `DbIntegrityJobStarted` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `MaintenanceJobStatus` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `DbBackupRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `DbBackupResult` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `ServiceLogsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ServiceLogsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ServiceJournalEntry` | runtime/collector state | Assigned to runtime/capability owner; excluded from domain. |
| `AiWatchStatusReport` | runtime/collector state | Assigned to runtime/capability owner; excluded from domain. |
| `IncidentRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `IncidentResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `IncidentEvent` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `LogEntry` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `HostStateRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HostStateResponse` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `FleetStateRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `FleetStateHostRow` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `FleetStateSummary` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `FleetStateResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `CorrelateStateRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `CorrelateStateWindow` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `CorrelateStateHostEntry` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `CorrelateStateResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `graph.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `GraphEntityLookupRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphAroundRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphExplainRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphProjectionStatusResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphRebuildStatsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphRebuildResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphEntity` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `GraphEntityCandidate` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `GraphRelationship` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `GraphEntitySummary` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `GraphEvidence` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `GraphEvidenceLookupRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphSourceLogSummary` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `GraphEvidenceLookupResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphNextQuery` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphResponseMetadata` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphEntityLookupResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphAroundResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphExplainResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GraphIncidentNarrative` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `GraphNarrativeChain` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |

## `hook_assess.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `HookAssessRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HookAssessResult` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `HookAssessResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `hook_events.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `HookBackfillRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HookBackfillResult` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ListHookEventsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HookEventEntry` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `ListHookEventsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `investigation.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `InvestigationVersionResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `InvestigationEnvelope` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `InvestigationMetadata` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `InvestigationBudget` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `InvestigationBudgetUsed` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `InvestigationClaimType` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `InvestigationClaim` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AppEntitySummary` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AppRelationshipSummary` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AppEvidenceSummary` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AppLogSummary` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AppGraphResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AppGraphEntityResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AppGraphEvidenceResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AskInvestigationRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AskInvestigationResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `log_query.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `SearchLogsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `FilterLogsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `SearchLogsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `TailLogsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ErrorSummaryEntry` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `GetErrorsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `GetErrorsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HostEntry` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `ListHostsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HomelabMapRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HomelabMapResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HomelabMapGraphAnswer` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HomelabMapGraphTarget` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HomelabMapAnswerRow` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HomelabMapAnswerTruncation` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HomelabMapNextQuery` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HomelabMapProofQuery` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `TopologyFinding` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `TopologyFindingEntity` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `TopologyFindingEvidence` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `HomelabMapSummary` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HomelabMapNode` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HomelabMapSourceIp` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `HomelabMapApp` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `CortexOverlaySummary` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `CorrelateEventsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `CorrelatedHost` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `CorrelateEventsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `mcp_assess.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `McpAssessRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `McpAssessResult` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `McpAssessResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `mcp_events.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `McpBackfillRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `McpBackfillResult` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ListMcpEventsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `McpEventEntry` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `ListMcpEventsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `ops.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `UnaddressedErrorsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `UnaddressedErrorsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ErrorSignatureEntry` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AckErrorRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AckErrorResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `UnackErrorRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `UnackErrorResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `NotificationsRecentRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `LlmInvocationsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiCheckpointsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiParseErrorsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AiPruneCheckpointsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `DbIntegrityRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `DbCheckpointRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `DbVacuumRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `rag.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `SimilarIncidentsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `CorrelatedSession` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `IncidentCluster` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `SimilarIncidentsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `IncidentContextRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `SeverityCount` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `AppLogCount` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `IncidentContextResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `skill_assess.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `SkillAssessRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `SkillAssessResult` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `SkillAssessResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `skill_events.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `SkillBackfillRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `SkillBackfillResult` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ListSkillEventsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `SkillEventEntry` | semantic contract | Extracted to `cortex-domain` with product/storage conversions omitted. |
| `ListSkillEventsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## `stats.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `DbStats` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `ListAppsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ListAppsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AppEntry` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `ListSourceIpsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `ListSourceIpsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `SourceIpHostBreakdown` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `SourceIpEntry` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `TimelineRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `TimelineResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `TimelinePoint` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |
| `PatternsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `PatternsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `PatternEntry` | storage/query projection | Assigned to `cortex-storage-sqlite` / application query adapter; excluded from domain. |

## `surface.rs`

| Type | Classification | Wave 1 disposition |
|---|---|---|
| `AnalysisRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AnalysisResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `CorrelateRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `CorrelateResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `StateRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `StateResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `StatsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `StatsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `IngestRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `IngestResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AlertsRequest` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |
| `AlertsResponse` | transport DTO/policy | Assigned to API/MCP/CLI/application boundary; excluded from domain. |

## Public re-exports not captured by declarations

`ops.rs` additionally re-exports `FileTailAddRequest`, `FileTailOp`, `FileTailRequest`, `FileTailResponse`, `FileTailSource`, and `FileTailStatus` from `crate::filetail`. The request/response/op shapes are transport DTOs and the source/status values are runtime/collector state. None belong in `cortex-domain`.

## Boundary violations found in the donor

- **53** `impl From<db::...>` mappings live beside public models. The extracted domain owns none of them; Wave 2 assigns them to `cortex-storage-sqlite`.
- `HostStateResponse`, `CorrelateStateHostEntry`, `GraphSessionCorrelation`, and topic-correlation payloads expose raw `db::Heartbeat* types. Wave 1 introduces domain-owned heartbeat contracts and uses those in extracted semantic aggregates.
- MCP and skill incident evidence expose raw `Vec<db::AiMcpEventEntry>` / `Vec<db::AiSkillEventEntry>`. Extracted evidence uses domain-owned `McpEventEntry` / `SkillEventEntry`.
- `AiWatchStatusReport` exposes `crate::scanner::AiIndexingHealth`; `DbStats::from` reads a `crate::receiver` process counter; `ops.rs` re-exports `crate::filetail`; `surface.rs` embeds `crate::config::NotificationsConfig`; and `log_query.rs` imports `crate::inventory::schema::* . Those stay with runtime, transport, or inventory owners.

## Error taxonomy ownership

The donor `ServiceError` mixes semantic failures with storage/runtime classification. `cortex-domain` extracts only `DomainError::InvalidInput` and `DomainError::NotFound`. SQLite busy/timeout, constraint violations, row-not-found persistence details, pool starvation, and opaque `anyhow` failures stay in application/storage adapters, which translate them at the surface boundary.

## Completeness check

The inventory was generated against all `pub struct`, `pub enum`, and `pub type` declarations under the donor `src/app/models/*.rs`; all **255** declarations are represented exactly once above. The six `filetail` re-exports are recorded separately because they are not declarations in that directory.
