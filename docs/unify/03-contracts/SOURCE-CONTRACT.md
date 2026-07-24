# Source and Generation Contract

## Purpose

This contract governs finite or refreshable knowledge sources. It MUST NOT be applied to continuing observation streams merely to force a single ingestion lifecycle.

## Lifecycle

```text
SourceRequest
    ↓ route
ResolvedSource
    ↓ register
Source
    ↓ discover
SourceManifest
    ↓ compare with committed baseline
SourceManifestDiff
    ↓ acquire added/changed items
SourceAcquisition
    ↓ normalize
SourceDocument[]
    ↓ prepare/index
Generation documents and points staged
    ↓ complete
GenerationComplete
    ↓ publish one authoritative committed pointer
GenerationPublished
    ↓ asynchronous cleanup debt
```

## Authority

- The source ledger owns source and generation state.
- Durable source content or canonical document rows own source truth.
- Qdrant and FTS records are derived indexes.
- A generation MUST NOT become query-visible until publication succeeds.
- A failed generation MUST NOT replace the previous committed generation.
- Unchanged items MAY be carried forward logically or physically, but the committed generation pointer remains singular.

## Stable identity

`sourceId` MUST derive from the canonical origin, source kind, and identity-affecting scope. Credentials, transient query parameters, local cache paths, and display names MUST NOT affect it.

`itemId` MUST remain stable across refreshes when the logical source item remains the same.

`documentId` MUST remain stable for the same source item and normalization partition.

`chunkId` MUST include the document identity, processing fingerprint, and stable chunk coordinates or content identity.

Any change to ID inputs requires migration documentation and compatibility fixtures.

## Manifest requirements

A manifest MUST:

- identify one source;
- use a versioned manifest format;
- state whether discovery is complete;
- contain unique item IDs;
- preserve item locator, content hash when available, metadata bounds, and source timestamps;
- distinguish unknown content hash from an empty item;
- never include credentials.

An incomplete manifest MUST NOT infer that absent prior items were removed.

## Diff semantics

- `added`: no corresponding committed item.
- `changed`: same logical item, acquisition-affecting fingerprint changed.
- `unchanged`: same logical item and processing remains reusable.
- `removed`: present in complete committed baseline, absent from complete new manifest.
- Categories MUST be disjoint.
- The union MUST equal all relevant baseline/new item identities.

A processing-fingerprint change MAY require re-preparing an unchanged source item without marking its source bytes changed.

## Generation state machine

```text
discovered → acquiring → preparing → staging → complete → published
                    ↘ failed     ↘ failed       ↘ failed
queued/running states may be cancelled before published
```

`published` is terminal. Cleanup is separate and idempotent.

## Lease behavior

- At most one active publisher lease exists for a source unless an explicit concurrent-generation policy is approved.
- Leases have owner, acquired time, heartbeat time, and expiry.
- Expired leases are recoverable.
- A worker MUST prove lease ownership before publication.
- Publication uses optimistic baseline validation to reject stale generations.

## Cleanup debt

Publication records cleanup work rather than synchronously deleting every derived artifact. Debt records MUST be:

- durable;
- idempotent;
- retryable;
- versioned by cleanup kind;
- tied to canonical source/generation identities;
- dead-letterable with bounded diagnostics.

## Cancellation

Cancellation before publication leaves the prior committed generation intact. Staged derived artifacts become cleanup debt. Cancellation after publication cannot roll back the authoritative pointer without an explicit rollback generation.

## Required fixtures

- new source;
- no-change refresh;
- added/changed/removed mix;
- incomplete discovery;
- processing fingerprint change;
- concurrent generations;
- lease expiry and recovery;
- crash between stage and publish;
- crash after publish before cleanup;
- idempotent retry.
