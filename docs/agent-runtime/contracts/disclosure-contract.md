---
title: "Progressive Disclosure Contract"
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

# Progressive Disclosure Contract

## Request

A <code>DisclosureRequest</code> contains:

- request, run, agent, and context IDs;
- requested level;
- purpose;
- selectors for domains, entities, evidence, findings, or canonical references;
- representation;
- record, byte, and token limits;
- raw-evidence flag;
- parent claim, question, snippet, or tool call;
- timestamp.

## Decision

A <code>DisclosureDecision</code> has status:

- <code>allowed</code>;
- <code>narrowed</code>;
- <code>denied</code>;
- <code>approval-required</code>.

It records granted level, selected items, omitted items, reason codes, policy references, authorization reference, budgets, sensitivity, and expiry.

## Receipt

An allowed or narrowed decision produces a <code>DisclosureReceipt</code> containing canonical references, materialization or resource handles, digests, sizes, classifications, and the exact representation supplied to the runtime.

## Invariants

- Disclosure never broadens effective capabilities.
- A mounted file is not considered disclosed until its content or path is supplied to the model or agent process through a recorded operation.
- A disclosed summary does not imply disclosure of every source record used to derive it.
- Raw sensitive evidence requires explicit policy.
- Denied selectors do not leak through counts, snippets, graph paths, errors, or catalog names when their existence is protected.
- Expired decisions cannot authorize new reads.
- Every receipt belongs to one decision and one context generation.

## Reason codes

Initial reason families:

- task relevance;
- saved view;
- snippet requirement;
- claim verification;
- dependent research;
- raw evidence required;
- source sensitivity;
- policy denial;
- authorization denial;
- budget narrowing;
- stale source;
- unavailable source;
- approval boundary.

## Operations

- <code>disclosure.request</code> creates and evaluates a request.
- <code>disclosure.approve</code> or <code>disclosure.deny</code> resolves an approval boundary.
- <code>disclosure.get</code> returns the decision and receipt subject to authorization.
- <code>disclosure.list</code> lists bounded run history.

## Errors

Errors include invalid level, selector, context generation, representation, budget, expired approval, and unauthorized access. Policy denials return a stable decision rather than an internal error.
