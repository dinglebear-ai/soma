# `soma-crawl`

**Proposed path:** `crates/shared/knowledge/crawl`  
**Delivery phase:** Knowledge ingestion  
**Publication:** Publishable.

## Purpose

Independent bounded web crawling and capture engine usable without RAG.

## Donor material

- Axon: Spider-based web_engine and crawl/scrape code
- Axon: browser bootstrap, screenshots, sitemaps and manifests

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- CrawlRequest and CrawlPolicy
- Scope and URL frontier
- Budgets and concurrency
- robots/sitemap policy
- HTTP and browser capture providers
- Page and asset capture
- Screenshot and WARC hooks
- Crawl manifests
- Crawl events and receipts

## Explicit exclusions

- Source generations
- Document chunking
- Vector writes
- Soma web UI
- Global browser credentials

## Public API candidates

- `Crawler`
- `CrawlRequest`
- `CrawlPolicy`
- `CrawlManifest`
- `CapturedPage`
- `CapturedAsset`
- `CrawlEvent`
- `CrawlReceipt`
- `BrowserProvider`
- `FetchProvider`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- soma-process
- soma-sanitize
- spider optional/default selected feature
- browser adapter optional

## Feature plan

- `http`
- `spider`
- `chrome`
- `screenshots`
- `warc`
- `sitemap`
- `serde`
- `schema`

Default features MUST remain minimal. Heavy providers, storage engines, platform collectors, and parser grammars are opt-in unless an ADR approves otherwise.

## Required behavior

1. All limits, clocks, paths, policies, and provider handles are explicit inputs.
2. Cancellation behavior is documented and tested.
3. Error classification distinguishes transient, permanent, invalid-input, unavailable-provider, and cancelled states where applicable.
4. Diagnostics are bounded and secret-safe.
5. Stable identifiers and serialized records have golden compatibility fixtures.
6. Implementations remain usable without Soma's CLI, API, MCP, web server, or global configuration.

## Verification

- local fixture sites
- redirect/scope tests
- budget enforcement
- robots/sitemap fixtures
- browser smoke tests
- screenshot artifact tests

## Initial Soma consumers

- soma-sources web feature
- future monitoring and archiving applications

## Extraction acceptance

```text
[ ] Donor paths and exact source baseline recorded
[ ] Neutral API accepted
[ ] Donor fixtures copied or recreated
[ ] Pure implementation moved
[ ] Product/config dependencies removed
[ ] Optional backend adapters implemented
[ ] Soma integration proves real use
[ ] External consumer fixture passes
[ ] Package contents reviewed
[ ] Publication gate passes
```

## Deferred work

Features not required by a v1 vertical slice remain deferred rather than represented by placeholder public APIs. The crate MUST NOT add APM, worker-agent, Incus mission, or Orchestrator concepts.
