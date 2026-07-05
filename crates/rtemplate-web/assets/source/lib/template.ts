import { ACTIONS } from "./generated-actions";

export const WEB_APP_CONFIG = {
  serviceName: "example",
  displayName: "rmcp-template",
  dashboardTitle: "Operator Dashboard",
  description: "MCP server operator dashboard",
  apiBaseUrl: process.env.NEXT_PUBLIC_RTEMPLATE_API_BASE_URL ?? "",
  capabilitiesEndpoint: "/v1/capabilities",
  healthEndpoint: "/health",
  statusEndpoint: "/status",
  mcpEndpoint: "/mcp",
} as const;

export type ActionParam = {
  name: string;
  label: string;
  type: "text";
  placeholder?: string;
  required: boolean;
  description: string;
};

export type ActionSpec = {
  id: string;
  label: string;
  description: string;
  scope: "example:read" | "example:write" | "public";
  transport: "rest" | "mcp-only";
  method?: "GET" | "POST";
  path?: string;
  params: readonly ActionParam[];
  example: {
    action: string;
    params: Record<string, unknown>;
  };
  response: Record<string, unknown>;
};

export { ACTIONS };

export type RestAction = Extract<(typeof ACTIONS)[number], { transport: "rest" }>;
export type RestActionId = RestAction["id"];

export const REST_ACTIONS = ACTIONS.filter((action) => action.transport === "rest") as RestAction[];
export const DEFAULT_REST_ACTION = REST_ACTIONS[0];

export function normalizeApiBaseUrl(apiBaseUrl: string): string {
  return apiBaseUrl.replace(/\/+$/, "");
}

export function endpoint(path: string): string {
  return `${normalizeApiBaseUrl(WEB_APP_CONFIG.apiBaseUrl)}${path}`;
}
