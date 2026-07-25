# GraphRAG Evaluation Plan

## Objective

Measure whether graph-aware retrieval materially improves grounded answers over hybrid RAG alone.

## Dataset

Build a versioned scenario suite containing:

- project/repository questions;
- service/host topology;
- deployment-linked incidents;
- source code + official documentation questions;
- AI-session history;
- cross-service operational failures;
- time/version-sensitive configuration;
- conflicting evidence;
- irrelevant graph neighborhoods.

Each scenario defines:

```text
question
authorized scope
store snapshot IDs
required evidence
optional useful evidence
forbidden evidence
required entities
required graph paths
unsupported claims
acceptable uncertainty
```

## Retrieval metrics

- Recall@K for required evidence.
- Precision@K.
- MRR/nDCG for ranked evidence.
- Entity-resolution accuracy.
- Required-path recall.
- Dead-citation rate.
- Evidence diversity.
- Temporal/version correctness.
- Unauthorized evidence leakage: must be zero.
- Context bytes/tokens per required evidence item.
- Query latency by lane and total.

## Answer metrics

- Citation coverage of material claims.
- Citation entailment.
- Unsupported claim count.
- Correct separation of observed/documented/implemented/inferred.
- Root-cause accuracy where a root cause is established.
- Explicit unknown accuracy.
- Actionability judged against scenario rubric.
- Contradiction handling.

## Ablations

For every north-star class, compare:

1. FTS only.
2. Dense only.
3. Dense + sparse.
4. FTS + dense + sparse.
5. Hybrid + graph.
6. Hybrid + graph + memory.
7. Hybrid + graph + temporal correlation.
8. Full stack with reranking and synthesis.

GraphRAG is accepted when it improves evidence/path/root-cause metrics without unacceptable latency or context growth.

## Evaluation artifacts

Every run stores:

- query contract;
- planner output;
- backend versions;
- retrieved candidates and scores;
- graph expansions;
- reranker result;
- context bundle;
- answer and classified claims;
- evaluator results;
- trace IDs.

## Quality gate

Exact numeric release thresholds are established after the first representative baseline. The release gate MUST be comparative and scenario-specific rather than chosen from synthetic vanity numbers.
