---
title: "Soma Agent Runtime Overview"
created: 2026-08-05
updated: 2026-08-05
doc_type: "overview"
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

# Overview

Soma is becoming a living runtime for agents that need a richer world than a repository and a prompt.

A Soma run can combine current source code, upstream repositories, documentation, issues, pull requests, releases, prior agent sessions, shell commands, OTLP telemetry, syslog, journald, dmesg, Docker logs, reverse-proxy traffic, authentication decisions, UniFi events, process state, and device heartbeats. Axon supplies refreshable knowledge and research. Cortex supplies continuous operational reality. Soma joins both through one evidence graph and compiles only the subgraph relevant to the current task.

The filesystem context pack is an ergonomic projection of that graph, not a second authority. The durable contract is the context manifest plus the immutable compiled-context manifest. Together they record what sources were eligible, what graph query was executed, what evidence was selected, what was disclosed to the agent, and what revisions and time windows were used.

Code Mode is the computation layer. Instead of placing thousands of raw records into one prompt, the agent can traverse entities, filter traces, correlate timelines, group errors, compare deployments, test hypotheses, and reduce datasets programmatically. When local evidence exposes a knowledge gap, the run may create a bounded dependent Axon research job, ingest its result, enrich the graph, and resume synthesis.

Snippets make those investigations reusable. A snippet is a versioned Code Mode program with declared inputs, skills, context requirements, tool requirements, output contract, and risk class. LABBY's existing snippet store and Soma's existing Code Mode host are the starting points. Snippets can be bundled by APM, resolved by Soma, and executed only through the run's effective capability set.

Progressive disclosure controls how much of the world the agent sees. A run starts with identity, task, acceptance criteria, repository state, available context domains, snippet catalog, skill catalog, and scoped tool catalog. It may then request summaries, graph neighborhoods, evidence bundles, raw records, or cross-repository expansion. Every disclosure is recorded so Cortex can answer what the agent knew when it formed a claim or took an action.

Agent stacks are the deployment unit. A stack binds an agent package, context manifest, snippets, LABBY loadout, Incus profile, mounts, resources, observability, retention, and completion policy. Soma resolves the stack like a Compose application, launches it in Incus, uses the existing Codex app-server client or another runtime adapter, and records the complete lifecycle.

APM remains the package manager. It installs and locks prompts, skills, agents, hooks, plugins, and MCP dependencies across clients. Soma consumes the resolved APM package and lockfile, adds runtime policy and context, and creates the execution environment. Installation and execution stay separate security planes.

The result is not a prompt wrapper. It is a reproducible, policy-scoped, evidence-backed agent workload whose inputs, capabilities, context disclosures, runtime behavior, resource usage, and outputs can all be inspected after the fact.
