---
title: "Snippet Contract"
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

# Snippet Contract

## Discovery

Snippet sources are resolved in deterministic precedence order:

1. run-inline snippets;
2. stack-local snippets;
3. APM-resolved package snippets;
4. user snippets under <code>SOMA_HOME/snippets</code>;
5. built-in snippets.

Duplicate name and version pairs are errors unless the source is byte-identical. Shadowing by name with different versions must be explicit in the resolved stack.

## Definition

A snippet definition contains:

- name and semantic version;
- description and tags;
- source kind, path, canonical reference, and digest;
- typed inputs;
- skill, context, tool, snippet, and platform requirements;
- risk class and approval class;
- budgets;
- output schema reference;
- executable JavaScript.

The executable code limit MUST preserve the current Code Mode maximum of 20 KiB unless the shared Code Mode contract changes. Markdown source may use the larger LABBY donor bound, but only extracted code executes.

## Resolution

<code>snippet.resolve</code> accepts name, optional version requirement, caller input, and run ID. It returns an immutable resolved definition with validated merged input and requirements.

Resolution MUST reject:

- invalid names or versions;
- unknown input fields when the definition forbids them;
- missing required input;
- input type mismatch;
- path traversal or symlink escape;
- missing skill, context, or tool requirements;
- capability or risk denial;
- digest mismatch;
- recursion or dependency cycles.

## Execution

<code>snippet.execute</code> accepts a resolved snippet ID, run ID, context ID, and execution budgets. The Code Mode runner enforces existing call, recursion, depth, resolve-byte, timeout, response, log, state, and artifact limits.

The host MUST pass run-scoped <code>CodeModeCaller</code>, <code>ToolScope</code>, execution ID, and step ordinal into every tool call.

## Output

The output is validated against the declared schema. It includes value, artifacts, evidence references, calls, logs, timing, budgets, warnings, and effective risk class.

## Promotion

<code>snippet.promote</code> requires explicit user action and trusted local context. It writes a new user snippet atomically, records provenance, and never overwrites an existing different snippet without explicit replacement semantics.

## Error codes

Required codes:

- <code>snippet_invalid</code>;
- <code>snippet_not_found</code>;
- <code>snippet_version_unsatisfied</code>;
- <code>snippet_input_invalid</code>;
- <code>snippet_requirement_missing</code>;
- <code>snippet_capability_denied</code>;
- <code>snippet_risk_denied</code>;
- <code>snippet_output_invalid</code>;
- existing Code Mode recursion, depth, resolve, budget, timeout, and tool errors.
