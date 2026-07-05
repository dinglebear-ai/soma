/**
 * Typed client for the rmcp-template REST API.
 *
 * Business actions use direct REST routes such as POST /v1/echo and
 * GET /v1/status. REST does not expose an action envelope.
 *
 * The base URL is relative (empty string) so the same binary serves
 * both the API and the web UI — no CORS or cross-origin config needed.
 */

import { endpoint, WEB_APP_CONFIG } from "@/lib/template";

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

/** Dispatch a template REST action through its direct route. */
export function callAction<T = unknown>(
  action: string,
  params: Record<string, unknown> = {},
): Promise<ApiResponse<T>> {
  switch (action) {
    case "greet":
      return postJson<T>("/v1/greet", params);
    case "echo":
      return postJson<T>("/v1/echo", params);
    case "status":
      return apiFetch<T>(endpoint("/v1/status"));
    case "help":
      return apiFetch<T>(endpoint("/v1/help"));
    default:
      return Promise.resolve({ error: `Unknown REST action: ${action}` });
  }
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
