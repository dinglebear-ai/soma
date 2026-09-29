---
title: "Agent Run Contract"
created: 2026-08-05
updated: 2026-08-05
doc_type: "contract"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Agent Run Contract

## Operations

- <code>stack.validate</code>: schema and semantic validation with no side effects.
- <code>stack.resolve</code>: produce immutable resolved stack and capability decisions.
- <code>stack.run</code>: create a durable run from a resolved stack or stack source.
- <code>run.get</code>: return state, summary, references, and terminal details.
- <code>run.list</code>: bounded filtered listing.
- <code>run.cancel</code>: request cooperative cancellation.
- <code>run.stop</code>: stop runtime according to policy.
- <code>run.approve</code>: resolve a pending approval.
- <code>run.retry</code>: create a child attempt with explicit retry policy.
- <code>run.artifacts</code>: list bounded artifact receipts.

## Durable state

A run record MUST contain:

- run and parent IDs;
- stack, service, and agent IDs;
- resolved-stack digest;
- package, context, loadout, and runtime references;
- state and state version;
- lease/owner and heartbeat when workers are used;
- attempt and retry information;
- timestamps;
- terminal outcome and error;
- output and artifact references;
- retention and cleanup state.

## State machine

Allowed core transitions:

~~~text
created -> resolving
resolving -> resolved | failed | cancelled
resolved -> provisioning | cancelled
provisioning -> bootstrapping | failed | cancelled
bootstrapping -> running | failed | cancelled
running -> awaiting-approval | verifying | failed | cancelled
awaiting-approval -> running | failed | cancelled
verifying -> finalizing | failed | cancelled
finalizing -> succeeded | failed | cleanup-failed
~~~

Stopping, snapshotting, retrying, and recovery are explicit substates or events. Terminal states are immutable.

## Worker semantics

Axon's unified jobs are the donor for leases, heartbeats, cancellation, watchdog recovery, starvation handling, artifacts, retention, and terminal publication. Agent runs MAY use that engine directly or adopt its contracts into Soma's product layer. A separate weaker queue MUST NOT be introduced.

## Idempotency and recovery

Every side-effectful phase has a durable step identity. Recovered workers replay recorded successful steps or safely reconcile external state. Incus create, gateway provisioning, package installation, context compilation, and artifact publication all require idempotent or compensating behavior.

## Outputs

A terminal success requires declared output schemas and verification conditions to pass. A model response alone is not success when the stack requires artifacts, tests, or structured synthesis.

## Failure taxonomy

At minimum:

- validation;
- resolution;
- authorization;
- package;
- context;
- gateway;
- provisioning;
- bootstrap;
- runtime;
- approval;
- timeout or budget;
- verification;
- cancellation;
- finalization;
- cleanup.

The public error includes a stable code and safe message. Detailed diagnostics remain in authorized artifacts and events.
