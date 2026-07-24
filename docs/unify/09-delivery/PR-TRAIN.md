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

## Rules

- Every PR has one capability ID.
- No “miscellaneous convergence” PR.
- Generated artifacts are updated in the same PR.
- Architecture exceptions require owner, reason, expiry, and issue.
- Temporary adapters have removal criteria.
- A PR does not add speculative APM/agent APIs.
