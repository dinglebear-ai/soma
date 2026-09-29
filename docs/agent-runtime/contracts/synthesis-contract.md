---
title: "Synthesis Contract"
created: 2026-08-05
updated: 2026-08-05
doc_type: "contract"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Synthesis Contract

## Request

A synthesis request contains:

- run and context generation IDs;
- task or question;
- output schema;
- allowed snippets and research policy;
- dependent depth and job limits;
- time, token, call, item, and byte budgets;
- source authority and diversity requirements;
- stopping conditions;
- caller authorization.

## Result

A result contains:

- synthesis ID and status;
- summary;
- claims;
- findings;
- evidence index;
- conflicts and rejected hypotheses;
- open questions;
- dependent research questions and jobs;
- context generations used;
- recommended actions;
- budget and timing report;
- truncation;
- verification status;
- narrative projections.

## Claim contract

Each claim has:

- claim ID;
- text;
- optional normalized subject, predicate, and object;
- status;
- confidence from zero to one;
- supporting evidence IDs;
- contradicting evidence IDs;
- authority, freshness, and source diversity;
- inference explanation;
- parent question or claim;
- supersession links.

A claim with no supporting evidence can only be <code>unknown</code>, <code>claimed</code>, or a clearly marked hypothesis.

## Research contract

Dependent questions record why they were created and which evidence triggered them. Completed research returns canonical sources, citations, findings, limitations, and job metadata. The answer is not inserted as unclassified prose.

## Determinism

Calculations, retrieval plans, graph traversals, and result assembly SHOULD be deterministic against pinned inputs. Model-generated narrative is not deterministic and must never be the canonical result.

## Verification

Verification checks:

- output schema;
- evidence existence and authorization;
- claim/evidence consistency;
- citation hydration;
- budget and depth compliance;
- required source diversity;
- unresolved critical conflicts;
- requested tests or external checks.

## Terminal statuses

- <code>completed</code>;
- <code>completed-with-unknowns</code>;
- <code>insufficient-evidence</code>;
- <code>budget-exhausted</code>;
- <code>approval-required</code>;
- <code>cancelled</code>;
- <code>failed</code>.
