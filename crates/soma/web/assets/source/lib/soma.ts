import { ACTIONS } from "./generated-actions";

export const WEB_APP_CONFIG = {
  serviceName: "soma",
  displayName: "Soma",
  dashboardTitle: "Drop-in RMCP runtime",
  description:
    "Drop-in RMCP runtime for provider-backed MCP tools, prompts, resources, and agent capabilities.",
  apiBaseUrl: process.env.NEXT_PUBLIC_SOMA_API_BASE_URL ?? "",
  capabilitiesEndpoint: "/v1/capabilities",
  healthEndpoint: "/health",
  statusEndpoint: "/status",
  mcpEndpoint: "/mcp",
} as const;

export type ActionParam = {
  name: string;
  label: string;
  type: "text" | "number";
  placeholder?: string;
  required: boolean;
  description: string;
};

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

export type ActionSpec = {
  id: string;
  label: string;
  description: string;
  scope: string;
  transport: "rest" | "mcp-only";
  method?: HttpMethod;
  path?: string;
  params: readonly ActionParam[];
  example: {
    action: string;
    params: Record<string, unknown>;
  };
  response: Record<string, unknown>;
  source?: "static" | "provider";
  providerName?: string;
  providerKind?: string;
};

export { ACTIONS };

export type RestAction = ActionSpec & { transport: "rest" };
export type RestActionId = string;

export const REST_ACTIONS = ACTIONS.filter(
  (action) => action.transport === "rest",
) as readonly RestAction[];
export const DEFAULT_REST_ACTION = REST_ACTIONS[0];

export type ProviderInspection = {
  schema_version: number;
  provider_fingerprint: string;
  providers: ProviderInspectionProvider[];
};

export type ProviderInspectionProvider = {
  name: string;
  kind: string;
  title?: string | null;
  tools: ProviderInspectionTool[];
  prompts?: ProviderInspectionPrompt[];
  resources?: ProviderInspectionResource[];
};

export type ProviderInspectionTool = {
  name: string;
  title?: string | null;
  description: string;
  input_schema?: unknown;
  output_schema?: unknown;
  scope?: string | null;
  surfaces?: {
    mcp?: boolean;
    rest?: boolean;
    cli?: boolean;
    palette?: boolean;
  };
  rest?: {
    enabled?: boolean;
    method?: string | null;
    path?: string | null;
  } | null;
  generic_rest?: {
    enabled?: boolean;
    method?: string | null;
    path?: string | null;
  } | null;
};

export type ProviderInspectionPrompt = {
  name: string;
  description: string;
  arguments_schema?: unknown;
};

export type ProviderInspectionResource = {
  name: string;
  uri_template: string;
  description: string;
  mime_type?: string | null;
};

export function normalizeApiBaseUrl(apiBaseUrl: string): string {
  return apiBaseUrl.replace(/\/+$/, "");
}

export function endpoint(path: string): string {
  return `${normalizeApiBaseUrl(WEB_APP_CONFIG.apiBaseUrl)}${path}`;
}

export function mergeProviderRestActions(
  staticActions: readonly RestAction[],
  inspection?: ProviderInspection | null,
): RestAction[] {
  const actions = [...staticActions];
  const existing = new Set(actions.map((action) => action.id));
  for (const action of providerRestActions(inspection)) {
    if (existing.has(action.id)) continue;
    existing.add(action.id);
    actions.push(action);
  }
  return actions;
}

export function providerRestActions(inspection?: ProviderInspection | null): RestAction[] {
  if (!inspection?.providers) return [];
  return inspection.providers.flatMap((provider) =>
    provider.tools
      .filter((tool) => tool.surfaces?.rest === true || tool.rest?.enabled === true)
      .map((tool) => providerToolToRestAction(provider, tool)),
  );
}

export function providerInventory(inspection?: ProviderInspection | null) {
  const mcpOnlyTools =
    inspection?.providers.flatMap((provider) =>
      provider.tools
        .filter((tool) => tool.surfaces?.mcp !== false && tool.surfaces?.rest !== true)
        .map((tool) => ({ ...tool, provider: provider.name })),
    ) ?? [];
  const prompts =
    inspection?.providers.flatMap((provider) =>
      (provider.prompts ?? []).map((prompt) => ({ ...prompt, provider: provider.name })),
    ) ?? [];
  const resources =
    inspection?.providers.flatMap((provider) =>
      (provider.resources ?? []).map((resource) => ({ ...resource, provider: provider.name })),
    ) ?? [];
  return { mcpOnlyTools, prompts, resources };
}

export function coerceParamValues(
  action: Pick<ActionSpec, "params">,
  paramValues: Record<string, string>,
): Record<string, unknown> {
  const params: Record<string, unknown> = {};
  for (const param of action.params) {
    const value = paramValues[param.name];
    if (!value?.trim()) continue;
    params[param.name] = param.type === "number" ? Number(value) : value.trim();
  }
  return params;
}

function providerToolToRestAction(
  provider: ProviderInspectionProvider,
  tool: ProviderInspectionTool,
): RestAction {
  return {
    id: tool.name,
    label: tool.title ?? tool.name,
    description: tool.description,
    scope: tool.scope ?? "public",
    transport: "rest",
    method: normalizeMethod(tool.generic_rest?.method ?? tool.rest?.method),
    path: tool.generic_rest?.path ?? tool.rest?.path ?? `/v1/tools/${tool.name}`,
    params: paramsFromSchema(tool.input_schema),
    example: {
      action: tool.name,
      params: {},
    },
    response: {},
    source: "provider",
    providerName: provider.name,
    providerKind: provider.kind,
  };
}

function normalizeMethod(method?: string | null): HttpMethod {
  const normalized = method?.toUpperCase();
  if (
    normalized === "GET" ||
    normalized === "POST" ||
    normalized === "PUT" ||
    normalized === "PATCH" ||
    normalized === "DELETE"
  ) {
    return normalized;
  }
  return "POST";
}

function paramsFromSchema(schema: unknown): ActionParam[] {
  if (!isRecord(schema) || !isRecord(schema.properties)) return [];
  const required = Array.isArray(schema.required)
    ? new Set(schema.required.filter((item): item is string => typeof item === "string"))
    : new Set<string>();
  return Object.entries(schema.properties).map(([name, value]) => {
    const property = isRecord(value) ? value : {};
    return {
      name,
      label: typeof property.title === "string" ? property.title : labelFromName(name),
      type: inputTypeFromSchema(property),
      placeholder: placeholderFromSchema(property),
      required: required.has(name),
      description: typeof property.description === "string" ? property.description : name,
    };
  });
}

function inputTypeFromSchema(schema: Record<string, unknown>): "text" | "number" {
  const type = Array.isArray(schema.type) ? schema.type[0] : schema.type;
  return type === "integer" || type === "number" ? "number" : "text";
}

function placeholderFromSchema(schema: Record<string, unknown>): string | undefined {
  if (typeof schema.default === "string" || typeof schema.default === "number") {
    return String(schema.default);
  }
  if (typeof schema.example === "string" || typeof schema.example === "number") {
    return String(schema.example);
  }
  return undefined;
}

function labelFromName(name: string): string {
  return name
    .split(/[_-]+/)
    .filter(Boolean)
    .map((part) => `${part.slice(0, 1).toUpperCase()}${part.slice(1)}`)
    .join(" ");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
