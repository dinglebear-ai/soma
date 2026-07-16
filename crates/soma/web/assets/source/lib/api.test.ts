import { afterEach, describe, expect, it, vi } from "vitest";
import { apiFetch, callRestAction, getProviderCatalog, parseJsonBody } from "./api";

describe("apiFetch", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns parsed JSON for successful responses", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify({ status: "ok" }), { status: 200 })),
    );

    await expect(apiFetch<{ status: string }>("/health")).resolves.toEqual({
      data: { status: "ok" },
    });
  });

  it("uses structured API error messages when available", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify({ error: "forbidden" }), { status: 403 })),
    );

    await expect(apiFetch("/v1/echo")).resolves.toEqual({ error: "forbidden" });
  });

  it("preserves HTTP status when an error body has no error field", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("Bad gateway", { status: 502 })),
    );

    await expect(apiFetch("/v1/echo")).resolves.toEqual({ error: "HTTP 502" });
  });

  it("normalizes thrown fetch failures", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new Error("connection refused");
      }),
    );

    await expect(apiFetch("/health")).resolves.toEqual({ error: "connection refused" });
  });
});

describe("parseJsonBody", () => {
  it("handles empty bodies", () => {
    expect(parseJsonBody("")).toEqual({});
    expect(parseJsonBody("   ")).toEqual({});
  });

  it("returns non-JSON text unchanged", () => {
    expect(parseJsonBody("not json")).toBe("not json");
  });
});

describe("provider catalog client", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("fetches the live provider catalog endpoint", async () => {
    const fetch = vi.fn(
      async () => new Response(JSON.stringify({ providers: [] }), { status: 200 }),
    );
    vi.stubGlobal("fetch", fetch);

    await expect(getProviderCatalog()).resolves.toEqual({ data: { providers: [] } });
    expect(fetch).toHaveBeenCalledWith("/v1/providers", undefined);
  });

  it("dispatches dynamic REST action routes", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({ ok: true }), { status: 200 }));
    vi.stubGlobal("fetch", fetch);

    await expect(
      callRestAction(
        { id: "summarize", method: "POST", path: "/v1/providers/summarize" },
        { text: "hello" },
      ),
    ).resolves.toEqual({ data: { ok: true } });
    expect(fetch).toHaveBeenCalledWith("/v1/providers/summarize", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text: "hello" }),
    });
  });

  it("serializes GET action params into the query string", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({ ok: true }), { status: 200 }));
    vi.stubGlobal("fetch", fetch);

    await expect(
      callRestAction({ id: "lookup", method: "GET", path: "/v1/lookup" }, { q: "hello world" }),
    ).resolves.toEqual({ data: { ok: true } });
    expect(fetch).toHaveBeenCalledWith("/v1/lookup?q=hello+world", undefined);
  });
});
