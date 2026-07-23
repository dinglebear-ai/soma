"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { echo, getHealth, getStatus, greet, type StatusResult, status } from "@/lib/api";
import { cn } from "@/lib/utils";

type HealthState = "ok" | "error" | "loading";

interface ActivityItem {
  id: number;
  time: string;
  action: string;
  result: string;
  ok: boolean;
}

const installSteps = [
  {
    label: "Install",
    command: "npx soma-rmcp mcp",
    description: "Start from the npm launcher while keeping the installed command as soma.",
  },
  {
    label: "Bring online",
    command: "soma serve",
    description: "Serve MCP, REST, and the embedded web UI from one runtime.",
  },
  {
    label: "Drop in providers",
    command: "SOMA_PROVIDER_DIR=providers",
    description: "Provider manifests and modules become generated tools, prompts, and metadata.",
  },
] as const;

const providerSurfaces = [
  {
    title: "Provider modules",
    value: "providers/",
    description: "Runtime-scanned provider files for tools, prompts, resources, and actions.",
  },
  {
    title: "Scaffold intent",
    value: "scaffold_intent",
    description: "MCP elicitation flow that collects setup intent for new provider work.",
  },
  {
    title: "Client prompts",
    value: "Claude / Codex / Gemini",
    description: "Generated plugin surfaces share the same provider-backed capability metadata.",
  },
  {
    title: "Action catalog",
    value: "GET /v1/help",
    description: "REST, CLI, and MCP surfaces stay aligned through generated action metadata.",
  },
] as const;

const trustSignals = [
  ["Package", "soma-rmcp"],
  ["Binary", "soma"],
  ["Image", "ghcr.io/dinglebear-ai/soma:0.4.6"],
  ["License", "MIT"],
  ["Publisher", "dinglebear.ai"],
  ["Surfaces", "MCP, CLI, REST, web, Docker, plugins"],
] as const;

const statusColor: Record<HealthState, string> = {
  ok: "var(--aurora-success)",
  error: "var(--aurora-error)",
  loading: "var(--aurora-text-muted)",
};

const statusLabel: Record<HealthState, string> = {
  ok: "Healthy",
  error: "Unreachable",
  loading: "Checking...",
};

export default function LandingPage() {
  const [health, setHealth] = useState<HealthState>("loading");
  const [serverStatus, setServerStatus] = useState<StatusResult | null>(null);
  const [activity, setActivity] = useState<ActivityItem[]>([]);
  const nextIdRef = useRef(1);

  const checkHealth = useCallback(async () => {
    const res = await getHealth();
    setHealth(res.data?.status === "ok" ? "ok" : "error");
  }, []);

  const checkStatus = useCallback(async () => {
    const res = await getStatus();
    if (res.data) setServerStatus(res.data);
  }, []);

  const refreshRuntime = useCallback(async () => {
    await Promise.all([checkHealth(), checkStatus()]);
  }, [checkHealth, checkStatus]);

  useEffect(() => {
    refreshRuntime();
    const interval = setInterval(checkHealth, 10_000);
    return () => clearInterval(interval);
  }, [checkHealth, refreshRuntime]);

  const addActivity = useCallback((action: string, result: string, ok: boolean) => {
    const id = nextIdRef.current++;
    const item: ActivityItem = { id, time: new Date().toLocaleTimeString(), action, result, ok };
    setActivity((prev) => [item, ...prev].slice(0, 8));
  }, []);

  const handleGreet = async () => {
    const res = await greet("Alice");
    addActivity("POST /v1/greet", res.data?.greeting ?? res.error ?? "error", !res.error);
  };

  const handleEcho = async () => {
    const res = await echo("Hello from Soma.");
    addActivity("POST /v1/echo", res.data?.echo ?? res.error ?? "error", !res.error);
  };

  const handleStatus = async () => {
    const res = await status();
    addActivity("GET /v1/status", res.data?.status ?? res.error ?? "error", !res.error);
  };

  return (
    <div className="aurora-page-shell -m-6 min-h-screen px-4 py-5 sm:px-6 lg:px-8">
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-8">
        <section className="grid min-h-[calc(100vh-7.5rem)] items-center gap-6 py-5 lg:grid-cols-[minmax(0,0.92fr)_minmax(440px,1.08fr)]">
          <div className="space-y-7">
            <div className="flex items-center gap-3">
              <span
                className="flex size-12 items-center justify-center rounded-[var(--aurora-radius-1)] border"
                style={{
                  background: "var(--aurora-control-surface)",
                  borderColor: "var(--aurora-border-strong)",
                  boxShadow: "var(--aurora-highlight-medium)",
                }}
              >
                <SomaMark size={32} />
              </span>
              <div className="grid gap-1">
                <p
                  className="font-display text-3xl font-extrabold leading-none sm:text-4xl"
                  style={{
                    color: "var(--aurora-text-primary)",
                    fontFamily: "var(--aurora-font-display)",
                    letterSpacing: 0,
                  }}
                >
                  Soma
                </p>
                <p className="aurora-text-meta">Drop-in RMCP runtime</p>
              </div>
            </div>

            <div className="max-w-3xl space-y-5">
              <h1
                className="text-4xl font-extrabold leading-[1.06] sm:text-5xl lg:text-6xl"
                style={{
                  color: "var(--aurora-text-primary)",
                  fontFamily: "var(--aurora-font-display)",
                  letterSpacing: 0,
                }}
              >
                Provider-backed agent capabilities from one Rust runtime.
              </h1>
              <p
                className="max-w-2xl text-base leading-7 sm:text-lg"
                style={{ color: "var(--aurora-text-muted)" }}
              >
                Drop in provider manifests or modules for tools, prompts, resources, and actions.
                Soma supplies MCP, CLI, REST, auth, setup, generated metadata, and the embedded web
                UI.
              </p>
            </div>

            <div className="flex flex-wrap gap-3">
              <Button type="button" size="lg" onClick={refreshRuntime}>
                Check live status
              </Button>
              <Button asChild size="lg" variant="neutral">
                <a href="/tools/">Open tool runner</a>
              </Button>
              <Button asChild size="lg" variant="ghost">
                <a href="/api/">View API routes</a>
              </Button>
            </div>

            <div
              className="grid gap-2 rounded-[var(--aurora-radius-2)] border p-3 sm:grid-cols-3"
              style={{
                background: "var(--aurora-panel-medium)",
                borderColor: "var(--aurora-border-default)",
                boxShadow: "var(--aurora-shadow-medium), var(--aurora-highlight-medium)",
              }}
            >
              <TrustMetric label="REST routes" value="4" />
              <TrustMetric label="MCP actions" value="6" />
              <TrustMetric label="Transports" value="stdio / HTTP" />
            </div>
          </div>

          <HeroRuntimePanel health={health} serverStatus={serverStatus} />
        </section>

        <section className="grid gap-4 md:grid-cols-3" aria-label="Install and bring online paths">
          {installSteps.map((step, index) => (
            <Card
              key={step.label}
              className={cn(index === 0 && "md:translate-y-2", index === 2 && "md:-translate-y-2")}
              style={{
                background: "var(--aurora-panel-strong)",
                borderColor: "var(--aurora-border-strong)",
                boxShadow: "var(--aurora-shadow-strong), var(--aurora-highlight-strong)",
              }}
            >
              <CardHeader>
                <div className="flex items-center justify-between gap-3">
                  <Badge variant={index === 0 ? "info" : index === 1 ? "success" : "rose"} dot>
                    {step.label}
                  </Badge>
                  <span className="aurora-text-meta">0{index + 1}</span>
                </div>
                <CardTitle>{step.label}</CardTitle>
                <CardDescription>{step.description}</CardDescription>
              </CardHeader>
              <CardContent>
                <code
                  className="block rounded-[8px] border px-3 py-2 text-[12px] leading-5"
                  style={{
                    background: "var(--aurora-control-surface)",
                    borderColor: "var(--aurora-border-default)",
                    color: "var(--aurora-accent-strong)",
                    fontFamily: "var(--aurora-font-mono)",
                  }}
                >
                  {step.command}
                </code>
              </CardContent>
            </Card>
          ))}
        </section>

        <section className="grid gap-5 lg:grid-cols-[0.78fr_1.22fr]">
          <Card
            style={{
              background: "var(--aurora-panel-strong)",
              borderColor: "var(--aurora-border-strong)",
              boxShadow: "var(--aurora-shadow-strong), var(--aurora-highlight-strong)",
            }}
          >
            <CardHeader>
              <CardTitle>Provider drop-in surface</CardTitle>
              <CardDescription>
                Soma turns provider files into consistent agent, REST, CLI, and metadata surfaces.
              </CardDescription>
            </CardHeader>
            <CardContent className="grid gap-3">
              {providerSurfaces.map((surface) => (
                <div
                  key={surface.title}
                  className="rounded-[var(--aurora-radius-1)] border p-3"
                  style={{
                    background: "var(--aurora-control-surface)",
                    borderColor: "var(--aurora-border-default)",
                  }}
                >
                  <div className="mb-1 flex flex-wrap items-center justify-between gap-2">
                    <p className="aurora-text-label">{surface.title}</p>
                    <span
                      className="aurora-text-code"
                      style={{ color: "var(--aurora-accent-pink-strong)" }}
                    >
                      {surface.value}
                    </span>
                  </div>
                  <p className="aurora-text-body-sm" style={{ color: "var(--aurora-text-muted)" }}>
                    {surface.description}
                  </p>
                </div>
              ))}
            </CardContent>
          </Card>

          <Card
            style={{
              background: "var(--aurora-panel-medium)",
              borderColor: "var(--aurora-border-strong)",
              boxShadow: "var(--aurora-shadow-strong), var(--aurora-highlight-strong)",
            }}
          >
            <CardHeader>
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <CardTitle>Try the compiled runtime</CardTitle>
                  <CardDescription>
                    These controls call the same REST actions shown in the API explorer.
                  </CardDescription>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button type="button" size="sm" onClick={handleGreet}>
                    Greet
                  </Button>
                  <Button type="button" size="sm" variant="neutral" onClick={handleEcho}>
                    Echo
                  </Button>
                  <Button type="button" size="sm" variant="rose" onClick={handleStatus}>
                    Status
                  </Button>
                </div>
              </div>
            </CardHeader>
            <CardContent>
              <div className="grid gap-2">
                {activity.length === 0 ? (
                  <RuntimeRow
                    action="Ready"
                    result="Run a REST action to see the response stream here."
                    ok
                  />
                ) : (
                  activity.map((item) => (
                    <RuntimeRow
                      key={item.id}
                      time={item.time}
                      action={item.action}
                      result={item.result}
                      ok={item.ok}
                    />
                  ))
                )}
              </div>
            </CardContent>
          </Card>
        </section>

        <section
          className="mb-6 rounded-[var(--aurora-radius-3)] border p-4"
          style={{
            background: "var(--aurora-panel-strong)",
            borderColor: "var(--aurora-border-strong)",
            boxShadow: "var(--aurora-shadow-strong), var(--aurora-highlight-strong)",
          }}
        >
          <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
            <div>
              <h2 className="aurora-text-section">Distribution metadata</h2>
              <p className="aurora-text-body-sm" style={{ color: "var(--aurora-text-muted)" }}>
                Published surfaces are generated from the same runtime identity.
              </p>
            </div>
            <Button asChild variant="ghost" size="sm">
              <a href="https://github.com/dinglebear-ai/soma">GitHub source</a>
            </Button>
          </div>
          <div className="grid gap-2 md:grid-cols-2 lg:grid-cols-3">
            {trustSignals.map(([label, value]) => (
              <div
                key={label}
                className="rounded-[8px] border px-3 py-2"
                style={{
                  background: "var(--aurora-control-surface)",
                  borderColor: "var(--aurora-border-default)",
                }}
              >
                <p className="aurora-text-meta">{label}</p>
                <p
                  className="aurora-text-code mt-1 break-words"
                  style={{ color: "var(--aurora-text-primary)" }}
                >
                  {value}
                </p>
              </div>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}

function HeroRuntimePanel({
  health,
  serverStatus,
}: {
  health: HealthState;
  serverStatus: StatusResult | null;
}) {
  return (
    <div
      className="rounded-[var(--aurora-radius-3)] border p-4"
      style={{
        background: "var(--aurora-panel-strong)",
        borderColor: "var(--aurora-border-strong)",
        boxShadow: "var(--aurora-shadow-strong), var(--aurora-highlight-strong)",
      }}
    >
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <SomaMark size={18} />
          <span className="aurora-text-label">Runtime console</span>
        </div>
        <Badge variant={health === "ok" ? "success" : health === "error" ? "error" : "neutral"} dot>
          {statusLabel[health]}
        </Badge>
      </div>

      <div className="grid gap-3">
        <div
          className="rounded-[var(--aurora-radius-2)] border p-4"
          style={{
            background: "var(--aurora-control-surface)",
            borderColor: "var(--aurora-border-default)",
          }}
        >
          <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
            <span className="aurora-text-code" style={{ color: "var(--aurora-accent-strong)" }}>
              http://127.0.0.1:40060/mcp
            </span>
            <span className="aurora-text-meta">streamable-http</span>
          </div>
          <div className="grid gap-2">
            <ConsoleLine prefix="$" value="soma status --json" />
            <ConsoleLine
              prefix=">"
              value={`health: ${statusLabel[health].toLowerCase()}`}
              color={statusColor[health]}
            />
            <ConsoleLine
              prefix=">"
              value={`api: ${serverStatus?.status ?? "waiting for /v1/status"}`}
              color={
                serverStatus?.status === "ok" ? "var(--aurora-success)" : "var(--aurora-text-muted)"
              }
            />
            <ConsoleLine prefix=">" value="surfaces: mcp, cli, rest, web, docker, plugins" />
            <ConsoleLine prefix=">" value="providers: manifests + modules loaded at runtime" />
          </div>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <RuntimeSurface label="MCP" value="/mcp" tone="info" />
          <RuntimeSurface label="REST" value="/v1/status" tone="success" />
          <RuntimeSurface label="CLI" value="soma help" tone="rose" />
          <RuntimeSurface label="Web" value="/tools + /api" tone="neutral" />
        </div>
      </div>
    </div>
  );
}

function SomaMark({ size = 28 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={Math.round(size * 1.06)}
      viewBox="0 0 48 51"
      fill="none"
      aria-label="Soma stacked-plane mark"
      role="img"
      style={{ flexShrink: 0 }}
    >
      <path d="M8 13L24 7L40 13L24 19Z" fill="var(--aurora-border-strong)" opacity="0.96" />
      <path d="M8 21L24 15L40 21L24 27Z" fill="var(--aurora-accent-deep)" opacity="0.92" />
      <path d="M8 29L24 23L40 29L24 35Z" fill="var(--aurora-accent-primary)" opacity="0.88" />
      <path d="M8 37L24 31L40 37L24 43Z" fill="var(--aurora-accent-strong)" opacity="0.9" />
    </svg>
  );
}

function TrustMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-[8px] px-3 py-2">
      <p className="aurora-text-meta">{label}</p>
      <p className="aurora-text-code mt-1" style={{ color: "var(--aurora-text-primary)" }}>
        {value}
      </p>
    </div>
  );
}

function ConsoleLine({
  prefix,
  value,
  color = "var(--aurora-text-muted)",
}: {
  prefix: string;
  value: string;
  color?: string;
}) {
  return (
    <div className="grid grid-cols-[1.25rem_1fr] gap-2">
      <span className="aurora-text-code" style={{ color: "var(--aurora-accent-primary)" }}>
        {prefix}
      </span>
      <span className="aurora-text-code break-words" style={{ color }}>
        {value}
      </span>
    </div>
  );
}

function RuntimeSurface({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: "info" | "success" | "rose" | "neutral";
}) {
  return (
    <div
      className="rounded-[var(--aurora-radius-1)] border p-3"
      style={{
        background: "var(--aurora-control-surface)",
        borderColor: "var(--aurora-border-default)",
      }}
    >
      <Badge variant={tone}>{label}</Badge>
      <p className="aurora-text-code mt-3" style={{ color: "var(--aurora-text-primary)" }}>
        {value}
      </p>
    </div>
  );
}

function RuntimeRow({
  time,
  action,
  result,
  ok,
}: {
  time?: string;
  action: string;
  result: string;
  ok: boolean;
}) {
  return (
    <div
      className="grid gap-2 rounded-[8px] border p-3 sm:grid-cols-[5.5rem_9rem_1fr]"
      style={{
        background: "var(--aurora-control-surface)",
        borderColor: "var(--aurora-border-default)",
      }}
    >
      <span
        className="aurora-text-code"
        style={{ color: ok ? "var(--aurora-success)" : "var(--aurora-error)" }}
      >
        {time ?? "local"}
      </span>
      <span className="aurora-text-code" style={{ color: "var(--aurora-accent-primary)" }}>
        {action}
      </span>
      <span
        className="aurora-text-body-sm break-words"
        style={{ color: "var(--aurora-text-primary)" }}
      >
        {result}
      </span>
    </div>
  );
}
