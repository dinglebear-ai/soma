# Context Broker

The Context Broker is a Soma product service, not a v1 publishable shared crate.

## Responsibilities

- validate `ContextQuery`;
- resolve named entities and aliases;
- select SQL, FTS, vector, graph, memory, and synthesis lanes;
- apply authorization filters consistently;
- hydrate exact canonical evidence;
- deduplicate and rerank;
- enforce byte, item, and token budgets;
- produce `ContextBundle`;
- record retrieval traces and citation coverage.

## Query modes

| Mode | Required in v1 | Use |
|---|---:|---|
| structured | yes | IDs, time ranges, host/service/severity filters |
| lexical | yes | exact text, stack traces, paths, commands |
| hybrid | yes | dense + sparse + lexical |
| local graph | yes | entity neighborhood and evidence |
| temporal investigation | yes | changes and events around an incident |
| global community | stretch | environment-wide themes |
| DRIFT-style | deferred | iterative graph exploration |

## Planner contract

The planner MUST be deterministic for explicit mode requests. In `auto` mode it MAY use rules or an LLM classifier, but the selected lanes and filters MUST be returned in the retrieval trace.

## Synthesis contract

Synthesis MUST:

- cite material claims;
- distinguish observed and inferred statements;
- surface missing evidence;
- preserve contradictory evidence;
- never claim a runtime action occurred unless canonical evidence exists.
