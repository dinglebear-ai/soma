---
title: "Context Filesystem Specification"
created: 2026-08-05
updated: 2026-08-05
doc_type: "spec"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Context Filesystem Specification

## Purpose

The context filesystem exposes a compiled context to humans and filesystem-oriented agents without making the filesystem the canonical data store.

## Stable container paths

The runtime SHOULD use stable paths independent of host usernames:

~~~text
/soma/docs/               authoritative global docs projection
/soma/context/            compiled-context projection
/soma/package/            resolved APM package
/workspace/               repository workspace
/run/                     run metadata and artifacts
~~~

## Projection modes

### Full read-only mount

The complete authoritative docs tree is mounted read-only. Only catalogs and selected summaries are disclosed in prompts. This is the recommended first implementation.

### Lazy materialization

Soma creates a run-specific tree containing only selected documents and evidence. Reads and materializations are recorded.

### Virtual context filesystem

A later implementation may resolve paths dynamically from Axon, Cortex, and the graph. It must remain bounded, permission-aware, and observable. FUSE is not required for the first two phases.

## Context tree

A materialized context SHOULD use:

~~~text
/soma/context/
  manifest.json
  briefing.md
  catalog.json
  docs/
  repositories/
  issues/
  pull-requests/
  sessions/
  commands/
  telemetry/
  timelines/
  evidence/
  graph/
  artifacts/
~~~

Entries may be files, symlinks, or generated indexes according to materialization mode.

## Global docs

One authoritative docs corpus MAY live under a host-managed root and be mounted as <code>/soma/docs</code>. Repository-local convenience symlinks may point into that tree only when:

- targets remain inside the approved docs root;
- source identity and revision are recorded;
- symlink escapes are rejected;
- the mount is read-only;
- a broken link is reported as stale context.

## Naming and addressing

Every projected file MUST map back to a canonical URI or reference. Friendly paths are not authoritative IDs. A sidecar index SHOULD map relative paths to canonical references, content hashes, source revisions, and disclosure state.

## Writes

The context tree is read-only. Agent outputs belong in <code>/run/artifacts</code> or Code Mode artifacts. Agent state belongs in its bounded state workspace. Repository writes belong in <code>/workspace</code> only when authorized.

## Large data

Raw large logs, traces, metrics, and transcripts SHOULD remain queryable handles rather than copied files. Materialization requires an explicit bounded request and produces an artifact receipt.

## Portability

A portable context pack MUST include the generated manifest, required evidence, digests, schemas, and enough content to replay without canonical stores. Reference-mode packs may contain unresolved canonical handles and must report that dependency.
