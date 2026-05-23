import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { __resetCsrfTokenForTesting, getCsrfToken, refreshCsrfToken } from "./csrf";

const SAMPLE_TOKEN = "abcdefghijklmnopqrstuvwxyz0123456789_-ABCDE"; // 43 chars

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

beforeEach(() => {
  __resetCsrfTokenForTesting();
  vi.restoreAllMocks();
});

afterEach(() => {
  __resetCsrfTokenForTesting();
});

describe("getCsrfToken", () => {
  test("returns null before any refresh", () => {
    expect(getCsrfToken()).toBeNull();
  });
});

describe("refreshCsrfToken", () => {
  test("hydrates cache from /auth/me on 200", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(jsonResponse({ csrf_token: SAMPLE_TOKEN }));

    const result = await refreshCsrfToken();

    expect(result).toBe(SAMPLE_TOKEN);
    expect(getCsrfToken()).toBe(SAMPLE_TOKEN);
    expect(fetchSpy).toHaveBeenCalledWith(
      "/auth/me",
      expect.objectContaining({
        credentials: "same-origin",
        headers: { Accept: "application/json" },
      }),
    );
  });

  test("does NOT send an AbortSignal when none passed", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(jsonResponse({ csrf_token: SAMPLE_TOKEN }));

    await refreshCsrfToken();

    const init = fetchSpy.mock.calls[0]?.[1];
    expect(init).toBeDefined();
    expect((init as RequestInit).signal).toBeUndefined();
  });

  test("forwards AbortSignal when provided", async () => {
    const controller = new AbortController();
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(jsonResponse({ csrf_token: SAMPLE_TOKEN }));

    await refreshCsrfToken(controller.signal);

    const init = fetchSpy.mock.calls[0]?.[1];
    expect((init as RequestInit).signal).toBe(controller.signal);
  });

  test("clears cache to null on 401", async () => {
    // Seed cache so we can prove the failure path resets it.
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse({ csrf_token: SAMPLE_TOKEN }));
    await refreshCsrfToken();
    expect(getCsrfToken()).toBe(SAMPLE_TOKEN);

    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(new Response(null, { status: 401 }));

    const result = await refreshCsrfToken();

    expect(result).toBeNull();
    expect(getCsrfToken()).toBeNull();
  });

  test("clears cache to null on 5xx", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("server down", { status: 503 }));

    expect(await refreshCsrfToken()).toBeNull();
    expect(getCsrfToken()).toBeNull();
  });

  test("clears cache when response body is malformed JSON", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response("not json", {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    expect(await refreshCsrfToken()).toBeNull();
    expect(getCsrfToken()).toBeNull();
  });

  test("clears cache when csrf_token is omitted", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(jsonResponse({ id: "user-1", role: "adult" }));

    expect(await refreshCsrfToken()).toBeNull();
    expect(getCsrfToken()).toBeNull();
  });

  test("treats csrf_token: null (Basic-auth session) as no token", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(jsonResponse({ csrf_token: null }));

    expect(await refreshCsrfToken()).toBeNull();
    expect(getCsrfToken()).toBeNull();
  });

  test("rejects non-string csrf_token (schema drift)", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(jsonResponse({ csrf_token: 12345 }));

    expect(await refreshCsrfToken()).toBeNull();
    expect(getCsrfToken()).toBeNull();
  });

  test("clears cache when fetch throws (network error)", async () => {
    // Seed.
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse({ csrf_token: SAMPLE_TOKEN }));
    await refreshCsrfToken();

    vi.spyOn(globalThis, "fetch").mockRejectedValueOnce(new TypeError("network down"));

    expect(await refreshCsrfToken()).toBeNull();
    expect(getCsrfToken()).toBeNull();
  });

  test("last successful refresh wins on consecutive calls", async () => {
    const second = "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"; // 43 Z
    vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ csrf_token: SAMPLE_TOKEN }))
      .mockResolvedValueOnce(jsonResponse({ csrf_token: second }));

    await refreshCsrfToken();
    expect(getCsrfToken()).toBe(SAMPLE_TOKEN);
    await refreshCsrfToken();
    expect(getCsrfToken()).toBe(second);
  });
});
