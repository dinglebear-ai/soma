---
title: "Snippet Specification"
created: 2026-08-05
updated: 2026-08-05
doc_type: "spec"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Snippet Specification

## Purpose

A snippet is a reusable, versioned Code Mode investigation or transformation. It packages technique, not ambient authority.

LABBY's current implementation is the donor for filesystem discovery, Markdown frontmatter, JavaScript extraction, input validation, promotion, listing, resolution, and removal. Soma's shared Code Mode host and runner remain the execution engine.

## Storage

The proposed Soma directories are:

~~~text
<SOMA_HOME>/snippets/          user-installed snippets
<binary or package>/snippets/  built-in snippets
<run>/resolved-snippets/       immutable run receipts, not executable authority
~~~

APM MAY install snippet files or packages into a resolved package tree. Soma MUST resolve all sources into one deterministic catalog and report shadowing or collisions.

## File format

Markdown is the preferred authoring format. It includes YAML frontmatter, explanatory prose, and exactly one executable JavaScript block. Bare JavaScript MAY remain supported for generated or internal snippets.

Required metadata:

- name;
- version;
- description;
- risk class;
- typed inputs;
- required skills;
- required context domains;
- required tools or tool classes;
- output schema reference;
- source and integrity metadata.

Optional metadata:

- tags;
- saved context view;
- minimum disclosure level;
- maximum runtime and call budgets;
- supported platforms;
- dependent snippet references;
- whether Axon research jobs may be created.

## Risk classes

- <code>read_only</code>: queries and analysis only.
- <code>artifact_write</code>: writes bounded run artifacts or state.
- <code>repository_write</code>: edits a declared repository workspace.
- <code>runtime_mutation</code>: restarts or changes services and containers.
- <code>infrastructure_mutation</code>: changes networks, devices, gateways, storage, or host state.

A snippet's risk class is a minimum. Called tools may raise the effective class. The run MUST reject execution when the effective class exceeds policy.

## Skills

A snippet declares skill dependencies by package identity and version requirement. Skills are instructions available to the agent or runtime; they are not executable permissions. The run records exact resolved skill hashes.

## Inputs

Inputs MUST be validated before code resolution. Supported initial types match current Code Mode primitives:

- string;
- number;
- boolean;
- JSON.

Schemas MAY add enum, array, object, duration, URI, entity ID, and context ID after the first vertical slice.

## Execution

The Code Mode host resolves snippet code and merged input. Existing snippet recursion, depth, resolve-count, resolved-byte, call, timeout, response, and log budgets MUST remain enforced.

A snippet MUST execute against the run's scoped host catalog. It cannot request a global catalog by name.

## Composition

Snippets MAY call other snippets. Composition MUST preserve:

- recursion and depth limits;
- parent-child execution IDs;
- accumulated risk class;
- capability intersection;
- input and output schema validation;
- evidence and artifact lineage.

## Output

A snippet returns a structured result and MAY write artifacts through the Code Mode artifact API. The result SHOULD include:

- findings;
- evidence references;
- conflicts;
- open questions;
- suggested dependent research;
- recommended actions;
- timeline or comparison data;
- truncation and budget information.

## Promotion

A successful inline Code Mode investigation MAY be promoted into a user snippet only through an explicit action. Promotion MUST capture source code, metadata, inputs, required capabilities, observed outputs, and provenance. LABBY's promotion flow is the donor.

## Evaluation

Every production snippet SHOULD include fixtures proving:

- valid and invalid inputs;
- expected tool and source use;
- denied capability behavior;
- bounded result shape;
- evidence preservation;
- deterministic output where synthesis is absent;
- no secret leakage.
