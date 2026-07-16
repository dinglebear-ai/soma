/**
 * Typed client for the soma REST API.
 *
 * Business actions use direct REST routes such as POST /v1/echo and
 * GET /v1/status. Dropped provider tools can also run through the generic
 * POST /v1/tools/{action} route when they do not declare a custom REST route.
 *
 * The base URL is relative (empty string) so the same binary serves
 * both the API and the web UI — no CORS or cross-origin config needed.
 */

import {
  type ActionSpec,
  endpoint,
  type ProviderInspection,
  REST_ACTIONS,
  WEB_APP_CONFIG,
} from "@/lib/soma";

export interface ApiResponse<T = unknown> {
  data?: T;
  error?: string;
}

export interface GreetResult {
  greeting: string;
  target: string;
  server?: string;
}

export interface EchoResult {
  echo: string;
}

export interface StatusResult {
  status: string;
  note?: string;
}

export interface HealthResult {
  status: string;
}

/** Shared fetch helper — handles JSON parsing and error normalisation. */
export async function apiFetch<T>(url: string, options?: RequestInit): Promise<ApiResponse<T>> {
  try {
    const res = await fetch(url, options);
    const text = await res.text();
    const json = parseJsonBody(text);
    if (!res.ok) {
      const error =
        isRecord(json) && typeof json.error === "string" ? json.error : `HTTP ${res.status}`;
      return { error };
    }
    return { data: json as T };
  } catch (e) {
    return { error: e instanceof Error ? e.message : "Network error" };
  }
}

export function parseJsonBody(text: string): unknown {
  if (!text.trim()) return {};
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function postJson<T>(path: string, body: Record<string, unknown>): Promise<ApiResponse<T>> {
  return apiFetch<T>(endpoint(path), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

/** Fetch the live provider catalog used by MCP/REST dispatch. */
export function getProviderCatalog(): Promise<ApiResponse<ProviderInspection>> {
  return apiFetch<ProviderInspection>(endpoint("/v1/providers"));
}

/** Dispatch any REST-exposed action through its advertised route. */
export function callRestAction<T = unknown>(
  action: Pick<ActionSpec, "id" | "method" | "path">,
  params: Record<string, unknown> = {},
): Promise<ApiResponse<T>> {
  if (!action.path) {
    return Promise.resolve({ error: `REST action has no route: ${action.id}` });
  }
  const method = action.method ?? "POST";
  if (method === "GET") {
    return apiFetch<T>(endpoint(pathWithQuery(action.path, params)));
  }
  return apiFetch<T>(endpoint(action.path), {
    method,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(params),
  });
}

function pathWithQuery(path: string, params: Record<string, unknown>): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null) continue;
    query.set(key, String(value));
  }
  const queryString = query.toString();
  return queryString ? `${path}?${queryString}` : path;
}

/** Dispatch a Soma REST action through its direct route. */
export function callAction<T = unknown>(
  action: string,
  params: Record<string, unknown> = {},
): Promise<ApiResponse<T>> {
  const spec = REST_ACTIONS.find((item) => item.id === action);
  return spec
    ? callRestAction<T>(spec, params)
    : Promise.resolve({ error: `Unknown REST action: ${action}` });
}

/** GET /health */
export function getHealth(): Promise<ApiResponse<HealthResult>> {
  return apiFetch<HealthResult>(endpoint(WEB_APP_CONFIG.healthEndpoint));
}

/** GET /status */
export function getStatus(): Promise<ApiResponse<StatusResult>> {
  return apiFetch<StatusResult>(endpoint(WEB_APP_CONFIG.statusEndpoint));
}

export const greet = (name?: string) => postJson<GreetResult>("/v1/greet", name ? { name } : {});

export const echo = (message: string) => postJson<EchoResult>("/v1/echo", { message });

export const status = () => apiFetch<StatusResult>(endpoint("/v1/status"));
