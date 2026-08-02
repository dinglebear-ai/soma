---
title: "Pull Request Train"
created: 2026-07-24
updated: 2026-07-31
---

# Pull Request Train

Each vertical slice uses a predictable train.

## PR 1: Contract and decision

- ADR or accepted spec;
- donor baseline/path map;
- schemas and fixtures;
- exact acceptance test;
- no broad implementation.

## PR 2: Shared core

- crate scaffold;
- models/traits/pure logic;
- typed errors;
- unit/property/fuzz tests;
- external consumer fixture.

## PR 3: Backend/adapters

- SQLite/Qdrant/TEI/Spider/platform integrations;
- contract conformance;
- failure/cancellation tests.

## PR 4: Soma composition

- domain/application/runtime modules;
- config;
- job runners;
- authorization;
- observability.

## PR 5: Surfaces

- CLI;
- REST/OpenAPI/client;
- MCP action;
- Aurora web page;
- shared progress/errors.

## PR 6: Parity and operations

- donor differential tests;
- migration/rebuild;
- health/doctor;
- backup/retention impact;
- package readiness.

## Stacked branch protocol

- Every PR is developed in a dedicated worktree under `~/workspace/soma/.worktrees`.
- The first branch is based on current `origin/main`, never an uncommitted main checkout.
- Each later branch is created from the branch immediately below it.
- Each PR targets the preceding branch until that branch merges.
- After a lower PR merges, the next PR is rebased or restacked onto updated `main` and its base is changed before merge.
- A stack records branch, worktree, base branch, capability, and PR number in the implementation tracker.
- No branch in a stack may silently absorb unrelated changes from another active stack.

## Rules

- Every PR has one capability ID.
- No "miscellaneous convergence" PR.
- Generated artifacts are updated in the same PR.
- Architecture exceptions require owner, reason, expiry, and issue.
- Temporary adapters have removal criteria.
- A PR does not add speculative APM/agent APIs.
