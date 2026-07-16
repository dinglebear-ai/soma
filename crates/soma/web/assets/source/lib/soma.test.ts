import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  ACTIONS,
  coerceParamValues,
  mergeProviderRestActions,
  normalizeApiBaseUrl,
  type ProviderInspection,
  providerInventory,
  REST_ACTIONS,
} from "./soma";

type OpenApiActionMetadata = {
  components: {
    schemas: {
      ActionName: {
        enum: string[];
      };
    };
  };
  "x-soma": {
    rest_actions: string[];
    mcp_only_actions: string[];
    direct_rest_routes: Record<string, { method: "GET" | "POST"; path: string }>;
  };
};

const here = dirname(fileURLToPath(import.meta.url));
const openApi = JSON.parse(
  readFileSync(resolve(here, "../../../docs/generated/openapi.json"), "utf8"),
) as OpenApiActionMetadata;

describe("Soma action metadata", () => {
  it("keeps REST actions aligned with generated OpenAPI metadata", () => {
    const webRestActions = REST_ACTIONS.map((action) => action.id);
    expect(webRestActions).toEqual(openApi.components.schemas.ActionName.enum);
    expect(webRestActions).toEqual(openApi["x-soma"].rest_actions);
  });

  it("keeps direct REST route metadata aligned with generated OpenAPI metadata", () => {
    const webRoutes = Object.fromEntries(
      REST_ACTIONS.map((action) => [
        action.id,
        {
          method: action.method,
          path: action.path,
        },
      ]),
    );
    expect(webRoutes).toEqual(openApi["x-soma"].direct_rest_routes);
  });

  it("keeps MCP-only actions aligned with generated OpenAPI metadata", () => {
    const webMcpOnlyActions = ACTIONS.filter((action) => action.transport === "mcp-only").map(
      (action) => action.id,
    );
    expect(webMcpOnlyActions).toEqual(openApi["x-soma"].mcp_only_actions);
  });

  it("does not duplicate action identifiers", () => {
    const ids = ACTIONS.map((action) => action.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe("normalizeApiBaseUrl", () => {
  it("removes one or more trailing slashes", () => {
    expect(normalizeApiBaseUrl("http://localhost:40060/")).toBe("http://localhost:40060");
    expect(normalizeApiBaseUrl("http://localhost:40060///")).toBe("http://localhost:40060");
  });

  it("preserves empty same-origin configuration", () => {
    expect(normalizeApiBaseUrl("")).toBe("");
  });
});

describe("provider catalog conversion", () => {
  const inspection: ProviderInspection = {
    schema_version: 1,
    provider_fingerprint: "sha256:test",
    providers: [
      {
        name: "dynamic",
        kind: "ai-sdk",
        tools: [
          {
            name: "summarize",
            title: "Summarize",
            description: "Summarize text.",
            input_schema: {
              type: "object",
              required: ["text"],
              properties: {
                text: { type: "string", description: "Text to summarize." },
                max_words: { type: "integer", default: 12 },
              },
            },
            surfaces: { mcp: true, rest: true },
            rest: { enabled: true, method: "POST", path: "/v1/providers/summarize" },
            generic_rest: { enabled: true, method: "POST", path: "/v1/tools/summarize" },
          },
          {
            name: "mcp_only",
            description: "MCP-only tool.",
            surfaces: { mcp: true, rest: false },
          },
          {
            name: "runtime_check",
            description: "Check runtime.",
            surfaces: { mcp: true, rest: true },
            generic_rest: { enabled: true, method: "POST", path: "/v1/tools/runtime_check" },
          },
        ],
        prompts: [{ name: "brief", description: "Brief prompt." }],
        resources: [
          {
            name: "note",
            uri_template: "soma://notes/{id}",
            description: "Note resource.",
          },
        ],
      },
    ],
  };

  it("merges REST-capable provider tools without duplicating static actions", () => {
    const actions = mergeProviderRestActions(REST_ACTIONS, inspection);
    const action = actions.find((item) => item.id === "summarize");

    expect(action?.path).toBe("/v1/tools/summarize");
    expect(action?.params.map((param) => [param.name, param.type, param.required])).toEqual([
      ["text", "text", true],
      ["max_words", "number", false],
    ]);
    expect(actions.filter((item) => item.id === "echo")).toHaveLength(1);
    expect(actions.find((item) => item.id === "runtime_check")?.path).toBe(
      "/v1/tools/runtime_check",
    );
  });

  it("separates MCP-only tools, prompts, and resources for inventory display", () => {
    const inventory = providerInventory(inspection);

    expect(inventory.mcpOnlyTools.map((item) => item.name)).toEqual(["mcp_only"]);
    expect(inventory.prompts.map((item) => item.name)).toEqual(["brief"]);
    expect(inventory.resources.map((item) => item.uri_template)).toEqual(["soma://notes/{id}"]);
  });

  it("coerces numeric parameters before dispatch", () => {
    const action = mergeProviderRestActions(REST_ACTIONS, inspection).find(
      (item) => item.id === "summarize",
    );

    expect(action).toBeDefined();
    if (!action) throw new Error("summarize action should exist");
    expect(coerceParamValues(action, { text: " hello ", max_words: "8" })).toEqual({
      text: "hello",
      max_words: 8,
    });
  });
});
