---
apiVersion: soma.dev/v1alpha1
kind: Snippet
metadata:
  name: trace-service-failure
  version: 1.0.0
  description: Correlate a service error across source, gateway, auth, container, host, and network evidence.
  tags:
    - incident
    - timeline
    - read-only
spec:
  riskClass: read-only
  inputs:
    service:
      type: string
      required: true
      description: Canonical service name.
    since:
      type: duration
      required: false
      description: Investigation window.
      default: 2h
  requires:
    skills:
      - name: root-cause-analysis
        version: "^1"
      - name: temporal-correlation
        version: "^1"
      - name: evidence-grading
        version: "^1"
    context:
      - repository-knowledge
      - live-operations
      - soma-runtime
    tools:
      - soma::context.timeline
      - soma::context.graph
      - soma::context.evidence
      - axon::research
    research: true
    minimumDisclosureLevel: evidence
  budgets:
    maxItems: 5000
    maxBytes: 33554432
    maxTokens: 50000
    maxCalls: 128
    timeoutMs: 600000
  output:
    contentType: application/json
    schema: ../schemas/synthesis-result.schema.json
    artifacts:
      - name: incident-timeline
        contentType: application/json
        required: true
  platforms:
    - linux
---

# Trace service failure

Build a cross-layer incident timeline, evaluate competing hypotheses, and return only findings supported by canonical evidence. Preserve contradictions and state what remains unknown.

~~~js
async () => {
  const service = input.service;
  const since = input.since || "2h";

  const results = await codemode.batch([
    () => codemode.soma.context_timeline({ service, since }),
    () => codemode.soma.context_graph({
      roots: [{ kind: "service", value: service }],
      relationships: [
        "changed-by",
        "emitted-by",
        "runs-on",
        "routes-to",
        "authenticated-as",
        "communicates-with"
      ],
      maxDepth: 4
    })
  ]);

  const timeline = results.ok.find((entry) => entry.i === 0)?.value ?? { events: [] };
  const graph = results.ok.find((entry) => entry.i === 1)?.value ?? { entities: [], relationships: [] };

  const evidence = await codemode.soma.context_evidence({
    service,
    timeline,
    graph,
    preserveConflicts: true
  });

  const unknownSignatures = (evidence.signatures || []).filter((signature) => !signature.known);
  const research = [];
  for (const signature of unknownSignatures.slice(0, 4)) {
    try {
      research.push(await codemode.axon.research({
        question: signature.question,
        dependsOn: signature.evidenceIds,
        sourcePolicy: "primary-first"
      }));
    } catch (error) {
      research.push({ ok: false, error: String(error) });
    }
  }

  const artifact = await writeArtifact(
    "incident-timeline.json",
    JSON.stringify({ service, since, timeline, graph, evidence, research }, null, 2),
    "application/json"
  );

  return {
    service,
    since,
    timelineSummary: evidence.timelineSummary,
    hypotheses: evidence.hypotheses,
    conflicts: evidence.conflicts,
    openQuestions: evidence.openQuestions,
    research,
    artifact
  };
}
~~~
