# Validation Report

**Generated:** 2026-07-22  
**Result:** PASS

## Package checks

| Check | Result |
|---|---|
| Package files | 112 |
| Markdown documents | 99 |
| Approximate documentation words | 27,074 |
| Proposed shared crates | 16 |
| `soma-<one-word>` naming | PASS |
| Deprecated compound crate references | 0 PASS |
| Per-crate specs present | PASS |
| Crate catalog matches specs | PASS |
| ADR count | 11 |
| Machine-readable capability entries | 13 |
| JSON/YAML/TOML parse | PASS |
| Draft 2020-12 schema validity | PASS |
| Representative schema fixtures | 4 PASS |
| Local Markdown links checked | 39 |
| Broken local Markdown links | 0 |
| Forbidden APM/Incus mission schema keys | 0 |
| Integrity checksum entries | 111 |

## Schema fixtures

- `source-request.json` validates as `SourceRequest`
- `observation-record.json` validates as `ObservationRecord`
- `graph-candidate.json` validates as `GraphCandidate`
- `context-query.json` validates as `ContextQuery`

## Crate-spec coverage

- `soma-primitives`
- `soma-sanitize`
- `soma-process`
- `soma-route`
- `soma-sources`
- `soma-crawl`
- `soma-ledger`
- `soma-jobs`
- `soma-llm`
- `soma-rag`
- `soma-transcript`
- `soma-memory`
- `soma-observations`
- `soma-ingest`
- `soma-collectors`
- `soma-graph`

## Entry-point coverage

`START-HERE.md` is the first implementation entry point and contains:

- the v1 scope and exclusions;
- the Soma capabilities already treated as authoritative;
- the separate Axon and Cortex ingestion semantics;
- the proposed crate catalog and first implementation batch;
- the local-knowledge walking-skeleton acceptance test;
- the ordered vertical-slice plan;
- five non-negotiable architectural rules.

## Naming contract

Every proposed public package uses the `soma-<one-word>` convention. The `soma-` portion is the namespace; the semantic package name contains no additional hyphen. Crates.io availability remains a pre-scaffolding gate.

## Scope check

The runtime schema bundle contains no Agent Package Manager, mission-lock, Incus worker-container, or custom-image contract. AI session and agent-event observation records remain valid because v1 ingests historical/runtime agent activity; it does not orchestrate or deploy agents.

## Integrity

`CHECKSUMS.sha256` covers every package file except itself. `MANIFEST.yaml` records package metadata, source baselines, scope, entry points, and per-file hashes for all non-self-referential content files.

## Source-level audit limitation

Repository cloning and compilation were not available during package generation. The source baselines and architecture were audited from the public repositories, but implementation MUST still:

- resolve full donor commit SHAs;
- run `cargo metadata`;
- verify feature unification;
- compile and test each donor fixture and extracted crate;
- inspect package contents and licenses;
- re-audit gateway PRs merged after the recorded Soma baseline.
