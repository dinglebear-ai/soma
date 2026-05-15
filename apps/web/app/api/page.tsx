"use client";

import { ACTIONS, WEB_APP_CONFIG } from "@/lib/template";

export default function ApiPage() {
  return (
    <div className="max-w-4xl mx-auto space-y-6">
      {/* Header */}
      <div>
        <h1
          style={{
            fontFamily: "var(--aurora-font-display)",
            fontSize: "1.75rem",
            fontWeight: 700,
            marginBottom: "0.25rem",
          }}
        >
          API Explorer
        </h1>
        <p style={{ color: "var(--aurora-text-muted)", fontSize: "0.875rem" }}>
          All surfaces (MCP, REST, CLI) call the same service methods.
        </p>
      </div>

      {/* Endpoint overview */}
      <div
        style={{
          background: "var(--aurora-panel-medium)",
          border: "1px solid var(--aurora-border-default)",
          borderRadius: "var(--radius-lg)",
          padding: "1.25rem",
        }}
      >
        <h2
          style={{
            color: "var(--aurora-text-muted)",
            fontSize: "0.75rem",
            fontWeight: 600,
            textTransform: "uppercase",
            letterSpacing: "0.05em",
            marginBottom: "1rem",
          }}
        >
          Endpoint
        </h2>
        <div className="space-y-2">
          <EndpointRow
            method="POST"
            path={WEB_APP_CONFIG.restEndpoint}
            description="REST action dispatch"
          />
          <EndpointRow
            method="GET"
            path={WEB_APP_CONFIG.healthEndpoint}
            description="Liveness probe (unauthenticated)"
          />
          <EndpointRow
            method="GET"
            path={WEB_APP_CONFIG.statusEndpoint}
            description="Runtime status"
          />
          <EndpointRow
            method="POST"
            path={WEB_APP_CONFIG.mcpEndpoint}
            description="MCP Streamable HTTP transport"
          />
          <EndpointRow
            method="GET"
            path="/openapi.json"
            description="Generated OpenAPI schema for the REST surface"
          />
        </div>
      </div>

      {/* Action parity table */}
      <div
        style={{
          background: "var(--aurora-panel-medium)",
          border: "1px solid var(--aurora-border-default)",
          borderRadius: "var(--radius-lg)",
          padding: "1.25rem",
        }}
      >
        <h2
          style={{
            color: "var(--aurora-text-muted)",
            fontSize: "0.75rem",
            fontWeight: 600,
            textTransform: "uppercase",
            letterSpacing: "0.05em",
            marginBottom: "1rem",
          }}
        >
          Surface Parity
        </h2>
        <div style={{ overflowX: "auto" }}>
          <table
            style={{
              width: "100%",
              borderCollapse: "collapse",
              fontSize: "0.8rem",
              fontFamily: "var(--aurora-font-mono)",
            }}
          >
            <thead>
              <tr style={{ borderBottom: "1px solid var(--aurora-border-default)" }}>
                {["Surface", "Call Pattern"].map((h) => (
                  <th
                    key={h}
                    style={{
                      textAlign: "left",
                      padding: "0.5rem 0.75rem",
                      color: "var(--aurora-text-muted)",
                      fontWeight: 600,
                      fontSize: "0.7rem",
                      textTransform: "uppercase",
                      letterSpacing: "0.05em",
                    }}
                  >
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {[
                ["MCP", `${WEB_APP_CONFIG.serviceName}(action="greet", name="Alice")`],
                [
                  "REST",
                  `POST ${WEB_APP_CONFIG.restEndpoint} {"action":"greet","params":{"name":"Alice"}}`,
                ],
                ["CLI", `${WEB_APP_CONFIG.serviceName} greet --name Alice`],
              ].map(([surface, pattern]) => (
                <tr
                  key={surface}
                  style={{ borderBottom: "1px solid var(--aurora-border-default)" }}
                >
                  <td
                    style={{
                      padding: "0.5rem 0.75rem",
                      color: "var(--aurora-accent-primary)",
                    }}
                  >
                    {surface}
                  </td>
                  <td
                    style={{
                      padding: "0.5rem 0.75rem",
                      color: "var(--aurora-text-muted)",
                      wordBreak: "break-all",
                    }}
                  >
                    {pattern}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      {/* Action reference */}
      <div className="space-y-4">
        {ACTIONS.map((action) => (
          <ActionCard key={action.id} action={action} />
        ))}
      </div>
    </div>
  );
}

function EndpointRow({
  method,
  path,
  description,
}: {
  method: string;
  path: string;
  description: string;
}) {
  const methodColors: Record<string, string> = {
    GET: "var(--aurora-success)",
    POST: "var(--aurora-accent-primary)",
  };

  return (
    <div
      style={{
        display: "flex",
        gap: "0.75rem",
        alignItems: "center",
        padding: "0.5rem 0.75rem",
        background: "var(--aurora-control-surface)",
        borderRadius: "var(--radius-md)",
        border: "1px solid var(--aurora-border-default)",
      }}
    >
      <span
        style={{
          color: methodColors[method] ?? "var(--aurora-text-muted)",
          fontFamily: "var(--aurora-font-mono)",
          fontSize: "0.7rem",
          fontWeight: 700,
          minWidth: "3rem",
        }}
      >
        {method}
      </span>
      <span
        style={{
          color: "var(--aurora-text-primary)",
          fontFamily: "var(--aurora-font-mono)",
          fontSize: "0.8rem",
          minWidth: "12rem",
        }}
      >
        {path}
      </span>
      <span style={{ color: "var(--aurora-text-muted)", fontSize: "0.8rem" }}>{description}</span>
    </div>
  );
}

function ActionCard({ action }: { action: (typeof ACTIONS)[number] }) {
  const isRestAction = action.transport === "rest";
  const curlExample = `curl -X POST http://localhost:3100${WEB_APP_CONFIG.restEndpoint} \\
  -H "Content-Type: application/json" \\
  -d '${JSON.stringify(action.example)}'`;

  return (
    <div
      style={{
        background: "var(--aurora-panel-medium)",
        border: "1px solid var(--aurora-border-default)",
        borderRadius: "var(--radius-lg)",
        padding: "1.25rem",
      }}
    >
      <div
        style={{ display: "flex", alignItems: "center", gap: "0.75rem", marginBottom: "0.5rem" }}
      >
        <span
          style={{
            background: "var(--aurora-hover-bg)",
            border: "1px solid var(--aurora-border-strong)",
            color: "var(--aurora-accent-primary)",
            fontFamily: "var(--aurora-font-mono)",
            fontSize: "0.85rem",
            fontWeight: 600,
            padding: "0.15rem 0.6rem",
            borderRadius: "var(--radius-sm)",
          }}
        >
          {action.id}
        </span>
        <span
          style={{
            color: isRestAction ? "var(--aurora-success)" : "var(--aurora-warn)",
            fontFamily: "var(--aurora-font-mono)",
            fontSize: "0.7rem",
            fontWeight: 600,
            textTransform: "uppercase",
            letterSpacing: "0.05em",
          }}
        >
          {isRestAction ? "REST + MCP + CLI" : "MCP only"}
        </span>
      </div>
      <p style={{ color: "var(--aurora-text-muted)", fontSize: "0.85rem", marginBottom: "1rem" }}>
        {action.description}
      </p>

      {action.params.length > 0 && (
        <div style={{ marginBottom: "1rem" }}>
          <p
            style={{
              color: "var(--aurora-text-muted)",
              fontSize: "0.7rem",
              fontWeight: 600,
              textTransform: "uppercase",
              letterSpacing: "0.05em",
              marginBottom: "0.5rem",
            }}
          >
            Parameters
          </p>
          {action.params.map((p) => (
            <div
              key={p.name}
              style={{
                display: "flex",
                gap: "0.5rem",
                alignItems: "baseline",
                fontSize: "0.8rem",
                fontFamily: "var(--aurora-font-mono)",
              }}
            >
              <span style={{ color: "var(--aurora-accent-pink)" }}>{p.name}</span>
              <span style={{ color: "var(--aurora-text-muted)" }}>string</span>
              {!p.required && (
                <span style={{ color: "var(--aurora-warn)", fontSize: "0.7rem" }}>optional</span>
              )}
              <span
                style={{ color: "var(--aurora-text-muted)", fontFamily: "var(--aurora-font-sans)" }}
              >
                — {p.description}
              </span>
            </div>
          ))}
        </div>
      )}

      <div className="space-y-3">
        {isRestAction ? (
          <CodeBlock label="cURL" code={curlExample} />
        ) : (
          <CodeBlock
            label="REST availability"
            code={`${action.id} is MCP-only because it requires an interactive MCP peer.`}
          />
        )}
        <CodeBlock
          label="MCP equivalent"
          code={`${WEB_APP_CONFIG.serviceName}(action="${action.id}"${action.params
            .map((p) => `, ${p.name}="..."`)
            .join("")})`}
        />
        <CodeBlock label="Response" code={JSON.stringify(action.response, null, 2)} />
      </div>
    </div>
  );
}

function CodeBlock({ label, code }: { label: string; code: string }) {
  return (
    <div>
      <p
        style={{
          color: "var(--aurora-text-muted)",
          fontSize: "0.7rem",
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: "0.05em",
          marginBottom: "0.25rem",
        }}
      >
        {label}
      </p>
      <pre
        style={{
          background: "var(--aurora-control-surface)",
          border: "1px solid var(--aurora-border-default)",
          borderRadius: "var(--radius-md)",
          padding: "0.75rem",
          color: "var(--aurora-accent-strong)",
          fontFamily: "var(--aurora-font-mono)",
          fontSize: "0.75rem",
          overflow: "auto",
          margin: 0,
          whiteSpace: "pre-wrap",
          wordBreak: "break-word",
        }}
      >
        {code}
      </pre>
    </div>
  );
}
