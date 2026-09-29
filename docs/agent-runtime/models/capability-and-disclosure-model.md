---
title: "Capability and Disclosure Model"
created: 2026-08-05
updated: 2026-08-05
doc_type: "model"
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

# Capability and Disclosure Model

## Capability lattice

Each policy layer produces a set of allowed upstreams, tools, scopes, context classes, mounts, networks, secrets, and mutation classes. Effective capability is intersection, not union.

~~~text
package request
  intersect stack policy
  intersect context policy
  intersect snippet requirements
  intersect LABBY policy
  intersect runtime profile
  intersect caller authorization
  = effective capabilities
~~~

A missing requested capability may be a validation error when required or a warning when optional.

## Mutation ordering

Mutation classes are ordered:

~~~text
read-only
< artifact-write
< repository-write
< runtime-mutation
< infrastructure-mutation
~~~

A snippet or tool raises, never lowers, the effective operation class. Approval policy can require a boundary at any class.

## Disclosure as separate authority

Capability answers “may the agent access this class of information or operation?” Disclosure answers “what was actually shown or exposed now?”

An agent can be authorized to query raw traces without receiving them during bootstrap. Conversely, a path mounted in the container is not automatically a model disclosure.

## Decision aggregate

A disclosure request and decision form one durable aggregate:

~~~text
request
  purpose
  selectors
  requested level and representation
  budget
  parent claim/question/snippet

decision
  status
  granted level
  selected and omitted items
  reason and policy references
  approval state

receipt
  exact representation
  canonical references
  digests and sizes
  expiry
~~~

## Catalog disclosure

Tools, snippets, skills, and context domains use the same model. A catalog may reveal only category and count at one level, names and descriptions at another, and exact schemas only when needed.

## Revocation

Capability or disclosure revocation prevents new operations. It cannot erase already observed model input, so revocation is recorded and future runs may need a new runtime or context session.

## Evaluation

Cortex can correlate disclosure sequences and capability use with run outcomes. This enables measurable context engineering rather than intuition-only prompt tuning.
