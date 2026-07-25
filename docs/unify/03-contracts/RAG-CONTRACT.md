# RAG Contract

## Purpose

The RAG crate transforms normalized `SourceDocument` or projected `IndexDocument` records into lexical and semantic indexes, then retrieves and optionally synthesizes cited results.

It does not acquire sources or own canonical observation storage.

## Index path

```text
SourceDocument / IndexDocument
    ↓ validate and redact
DocumentPreparer
    ↓ classify
ChunkRouter
    ↓ chunk/parse
PreparedChunk[]
    ↓ embedding batches
dense and optional sparse vectors
    ↓ deterministic point construction
stage/upsert
    ↓ index receipt
```

## Chunk routing

Routing considers, in order:

1. explicit content-kind hint;
2. trusted content type;
3. filename/path and extension;
4. bounded content inspection;
5. prose fallback.

Required v1 profiles:

- Markdown sections;
- code symbols/windows;
- prose windows;
- structured records;
- transcripts and AI-session turns;
- tool output;
- API schemas;
- atomic metadata.

Code parser support required for v1:

- Rust;
- Python;
- JavaScript;
- TypeScript/TSX.

Other languages MAY use heuristic code chunking until grammar support is added.

## Chunk invariants

A chunk MUST:

- reference one canonical document;
- have a stable ID and zero-based index;
- retain an exact source locator;
- carry a content hash and processing fingerprint;
- preserve meaningful section/symbol context;
- fit configured byte/token limits;
- avoid overlapping without explicit policy;
- contain no unredacted prohibited secret material.

A chunk MUST be citable back to canonical content without relying on Qdrant payload text.

## Processing fingerprint

The processing fingerprint MUST reflect all behavior that changes retrieval units:

- preparer/chunker identity and version;
- parser version and language grammar;
- redaction-policy version;
- metadata-policy version;
- relevant content normalization;
- embedding profile when point reuse depends on it.

A fingerprint change triggers deterministic reprocessing.

## Embedding

The embedder contract exposes:

- model identity;
- vector dimensions;
- dense/sparse capability;
- maximum inputs/tokens;
- batching constraints;
- usage;
- retry classification;
- cancellation.

Embeddings are derived and rebuildable.

## Vector point identity

Point ID MUST derive from:

```text
canonical record/document ID
+ projection kind
+ projection version
+ chunk index or stable chunk ID
+ embedding profile
```

Retries MUST be idempotent upserts.

## Query path

```text
QueryRequest
    ├── lexical FTS lane
    ├── dense vector lane
    ├── sparse vector lane
    └── supplied graph/memory candidates
             ↓
        fusion/deduplication
             ↓
           reranking
             ↓
       canonical hydration
             ↓
       citation/context budget
             ↓
       optional synthesis
```

## Retrieval scoring

The result MUST retain score components where available:

- FTS/BM25;
- dense similarity;
- sparse similarity;
- graph relevance;
- temporal relevance;
- authority/trust;
- reranker score.

The fused score is not evidence authority. A highly relevant untrusted record remains untrusted.

## Synthesis

Synthesis is optional.

When enabled:

- material claims require citations;
- observations, documentation, implementation facts, and inferences remain distinguishable;
- missing evidence is stated;
- retrieved text is treated as untrusted input;
- the model is not allowed to reinterpret a citation as stronger authority than its source class;
- the raw result bundle remains available independently of the prose answer.

## Backend requirements

Core APIs MUST remain usable with caller-supplied implementations. Qdrant, TEI, and specific rerankers are optional adapters.

## Required evaluation

- chunk boundary golden tests;
- retrieval relevance dataset;
- exact citation resolution;
- no-dead-citation tests;
- secret leakage tests;
- processing-fingerprint rebuild;
- dense/sparse/FTS ablations;
- reranker comparison;
- synthesis faithfulness and unsupported-claim checks.
