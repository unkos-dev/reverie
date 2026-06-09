import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { __resetCsrfTokenForTesting } from "./csrf";
import { ApiError } from "./errors";
import { apiFetch } from "./fetch";

const SAMPLE_TOKEN = "abcdefghijklmnopqrstuvwxyz0123456789_-ABCDE"; // 43 chars
const REFRESHED_TOKEN = "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"; // 43 Z

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

function problemResponse(
  problem: { type: string; title: string; status: number; detail?: string },
  init?: ResponseInit,
): Response {
  return new Response(JSON.stringify(problem), {
    status: problem.status,
    headers: { "Content-Type": "application/problem+json" },
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

async function seedCsrf(token: string): Promise<void> {
  const fetchSpy = vi
    .spyOn(globalThis, "fetch")
    .mockResolvedValueOnce(jsonResponse({ csrf_token: token }));
  const mod = await import("./csrf");
  await mod.refreshCsrfToken();
  fetchSpy.mockRestore();
}

describe("apiFetch — credentials + method normalisation", () => {
  test("GET request always sets credentials=same-origin", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books");

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const init = fetchSpy.mock.calls[0]?.[1];
    expect(init?.credentials).toBe("same-origin");
    expect(init?.method).toBe("GET");
  });

  test("uppercases the method", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books/1", { method: "patch" });

    const init = fetchSpy.mock.calls[0]?.[1];
    expect(init?.method).toBe("PATCH");
  });
});

describe("apiFetch — CSRF header injection", () => {
  test("GET does NOT inject X-CSRF-Token even when token is cached", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books");

    const headers = new Headers(fetchSpy.mock.calls[0]?.[1]?.headers);
    expect(headers.has("X-CSRF-Token")).toBe(false);
  });

  test("POST injects X-CSRF-Token when token is cached", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books", { method: "POST", body: "{}" });

    const headers = new Headers(fetchSpy.mock.calls[0]?.[1]?.headers);
    expect(headers.get("X-CSRF-Token")).toBe(SAMPLE_TOKEN);
  });

  test.each(["PUT", "PATCH", "DELETE"])("%s injects X-CSRF-Token", async (method) => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books/1", { method });

    const headers = new Headers(fetchSpy.mock.calls[0]?.[1]?.headers);
    expect(headers.get("X-CSRF-Token")).toBe(SAMPLE_TOKEN);
  });

  test("POST with no cached token omits header (rather than sending empty)", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books", { method: "POST" });

    const headers = new Headers(fetchSpy.mock.calls[0]?.[1]?.headers);
    expect(headers.has("X-CSRF-Token")).toBe(false);
  });
});

describe("apiFetch — RFC 7807 error parsing", () => {
  test("401 unauthorized → ApiError with parsed type/title/detail", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      problemResponse({
        type: "https://reverie.example/probs/unauthorized",
        title: "Unauthorized",
        status: 401,
        detail: "Authentication required.",
      }),
    );

    const err = await apiFetch("/api/v1/books").catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    const apiErr = err as ApiError;
    expect(apiErr.status).toBe(401);
    expect(apiErr.type).toBe("https://reverie.example/probs/unauthorized");
    expect(apiErr.title).toBe("Unauthorized");
    expect(apiErr.detail).toBe("Authentication required.");
    expect(apiErr.problemSlug).toBe("unauthorized");
  });

  test("500 internal → ApiError with title='Internal Server Error'", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      problemResponse({
        type: "https://reverie.example/probs/internal",
        title: "Internal Server Error",
        status: 500,
        detail: "An internal error occurred.",
      }),
    );

    const err = (await apiFetch("/api/v1/books").catch((e: unknown) => e)) as ApiError;
    expect(err.status).toBe(500);
    expect(err.title).toBe("Internal Server Error");
  });

  test("non-JSON error body falls back to status-text title", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response("Gateway down", {
        status: 502,
        statusText: "Bad Gateway",
        headers: { "Content-Type": "text/plain" },
      }),
    );

    const err = (await apiFetch("/api/v1/books").catch((e: unknown) => e)) as ApiError;
    expect(err.status).toBe(502);
    expect(err.title).toBe("Bad Gateway");
    expect(err.type).toBeNull();
    expect(err.problemSlug).toBeNull();
  });
});

describe("apiFetch — csrf-mismatch retry", () => {
  test("403 csrf-mismatch triggers refreshCsrfToken() once and retries", async () => {
    await seedCsrf(SAMPLE_TOKEN);

    const fetchSpy = vi.spyOn(globalThis, "fetch");
    // Attempt 1: mutating request → 403 csrf-mismatch.
    fetchSpy.mockResolvedValueOnce(
      problemResponse({
        type: "https://reverie.example/probs/csrf-mismatch",
        title: "Forbidden",
        status: 403,
        detail: "CSRF token invalid.",
      }),
    );
    // Refresh: /auth/me responds with the rotated token.
    fetchSpy.mockResolvedValueOnce(jsonResponse({ csrf_token: REFRESHED_TOKEN }));
    // Retry: succeeds.
    fetchSpy.mockResolvedValueOnce(jsonResponse({ ok: true }));

    const result = await apiFetch("/api/v1/books", { method: "POST" });

    expect(result).toEqual({ ok: true });
    expect(fetchSpy).toHaveBeenCalledTimes(3);
    // First attempt used the seeded token.
    expect(new Headers(fetchSpy.mock.calls[0]?.[1]?.headers).get("X-CSRF-Token")).toBe(
      SAMPLE_TOKEN,
    );
    // /auth/me was hit between attempts.
    expect(fetchSpy.mock.calls[1]?.[0]).toBe("/auth/me");
    // Retry used the refreshed token.
    expect(new Headers(fetchSpy.mock.calls[2]?.[1]?.headers).get("X-CSRF-Token")).toBe(
      REFRESHED_TOKEN,
    );
  });

  test("403 with a NON-csrf-mismatch problem type does not retry", async () => {
    await seedCsrf(SAMPLE_TOKEN);

    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      problemResponse({
        type: "https://reverie.example/probs/forbidden",
        title: "Forbidden",
        status: 403,
        detail: "Access denied.",
      }),
    );

    const err = (await apiFetch("/api/v1/books", { method: "POST" }).catch(
      (e: unknown) => e,
    )) as ApiError;
    expect(err.status).toBe(403);
    expect(err.problemSlug).toBe("forbidden");
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });

  test("GET that returns 403 does not attempt csrf-mismatch retry", async () => {
    // GETs are not gated by the csrf middleware on the backend; even
    // if a 403 csrf-mismatch shape came back on a GET, treating it as
    // a retryable case would be wrong.
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      problemResponse({
        type: "https://reverie.example/probs/csrf-mismatch",
        title: "Forbidden",
        status: 403,
      }),
    );

    const err = (await apiFetch("/api/v1/books").catch((e: unknown) => e)) as ApiError;
    expect(err.status).toBe(403);
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });
});

describe("apiFetch — body decoding", () => {
  test("204 No Content returns undefined without parsing", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(new Response(null, { status: 204 }));

    const result = await apiFetch("/api/v1/books/1", { method: "DELETE" });
    expect(result).toBeUndefined();
  });

  test("205 Reset Content returns undefined without parsing", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(new Response(null, { status: 205 }));

    const result = await apiFetch("/api/v1/books/1", { method: "DELETE" });
    expect(result).toBeUndefined();
  });

  test("204 on csrf-mismatch retry path returns undefined (does not call .json())", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    // Attempt 1: 403 csrf-mismatch.
    fetchSpy.mockResolvedValueOnce(
      problemResponse({
        type: "https://reverie.example/probs/csrf-mismatch",
        title: "Forbidden",
        status: 403,
      }),
    );
    // /auth/me refresh.
    fetchSpy.mockResolvedValueOnce(jsonResponse({ csrf_token: REFRESHED_TOKEN }));
    // Retry: succeeds with 204 (typical for DELETE after rotation).
    fetchSpy.mockResolvedValueOnce(new Response(null, { status: 204 }));

    const result = await apiFetch("/api/v1/books/1", { method: "DELETE" });
    expect(result).toBeUndefined();
    expect(fetchSpy).toHaveBeenCalledTimes(3);
  });
});
