import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __resetCsrfTokenForTesting } from "@/api/csrf";
import { __resetEtagCacheForTesting } from "@/api/etags";
import { ApiError } from "@/api/errors";
import { apiFetch } from "@/api/fetch";

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
  __resetEtagCacheForTesting();
  vi.restoreAllMocks();
});

afterEach(() => {
  __resetCsrfTokenForTesting();
  __resetEtagCacheForTesting();
});

async function seedCsrf(token: string): Promise<void> {
  const fetchSpy = vi
    .spyOn(globalThis, "fetch")
    .mockResolvedValueOnce(jsonResponse({ csrf_token: token }));
  const mod = await import("@/api/csrf");
  await mod.refreshCsrfToken();
  fetchSpy.mockRestore();
}

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

  test("POST with no cached token hydrates once; still omits the header when none is issued", async () => {
    // First response serves the lazy /auth/me hydration (no token for an
    // anonymous session), second the POST itself.
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({}))
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books", { method: "POST" });

    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(fetchSpy.mock.calls[0]?.[0]).toBe("/auth/me");
    const headers = new Headers(fetchSpy.mock.calls[1]?.[1]?.headers);
    expect(headers.has("X-CSRF-Token")).toBe(false);
  });
});

describe("apiFetch — csrf-missing retry", () => {
  test("428 csrf-missing refreshes once and retries with the new token", async () => {
    // A stale cached token the server no longer recognises as present
    // (session rotation cleared it server-side).
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(
        problemResponse({
          type: "https://reverie.example/probs/csrf-missing",
          title: "Precondition Required",
          status: 428,
        }),
      )
      .mockResolvedValueOnce(jsonResponse({ csrf_token: REFRESHED_TOKEN }))
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books", { method: "POST", body: "{}" });

    expect(fetchSpy).toHaveBeenCalledTimes(3);
    expect(fetchSpy.mock.calls[1]?.[0]).toBe("/auth/me");
    const retriedHeaders = new Headers(fetchSpy.mock.calls[2]?.[1]?.headers);
    expect(retriedHeaders.get("X-CSRF-Token")).toBe(REFRESHED_TOKEN);
  });

  test("a non-CSRF 428 throws without a retry", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      problemResponse({
        type: "https://reverie.example/probs/some-other-precondition",
        title: "Precondition Required",
        status: 428,
      }),
    );

    await expect(apiFetch("/api/v1/books", { method: "POST", body: "{}" })).rejects.toThrow(
      ApiError,
    );
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });
});

describe("apiFetch — RFC 9457 error parsing", () => {
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
});

describe("apiFetch — body decoding", () => {
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

function withEtag(body: unknown, etag: string): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json", ETag: etag },
  });
}

function mismatchResponse(currentEtag: string): Response {
  return new Response(
    JSON.stringify({
      type: "https://reverie.example/probs/if-match-mismatch",
      title: "Precondition Failed",
      status: 412,
    }),
    {
      status: 412,
      headers: { "Content-Type": "application/problem+json", ETag: currentEtag },
    },
  );
}

describe("apiFetch — If-Match ETag retention", () => {
  test("a GET response's ETag is echoed as If-Match on that resource's PATCH", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(withEtag({ status: null }, '"etag-1"'))
      .mockResolvedValueOnce(withEtag({ status: "reading" }, '"etag-2"'));

    await apiFetch("/api/v1/books/book-1/reading");
    await apiFetch("/api/v1/books/book-1/reading", {
      method: "PATCH",
      body: JSON.stringify({ status: "reading" }),
    });

    const patchHeaders = new Headers(fetchSpy.mock.calls[1]?.[1]?.headers);
    expect(patchHeaders.get("If-Match")).toBe('"etag-1"');
  });

  test("a successful PATCH's own ETag replaces the retained tag for the next PATCH", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(withEtag({ status: null }, '"etag-1"'))
      .mockResolvedValueOnce(withEtag({ status: "reading" }, '"etag-2"'))
      .mockResolvedValueOnce(withEtag({ status: "read" }, '"etag-3"'));

    await apiFetch("/api/v1/books/book-1/reading");
    await apiFetch("/api/v1/books/book-1/reading", {
      method: "PATCH",
      body: JSON.stringify({ status: "reading" }),
    });
    await apiFetch("/api/v1/books/book-1/reading", {
      method: "PATCH",
      body: JSON.stringify({ status: "read" }),
    });

    const secondPatchHeaders = new Headers(fetchSpy.mock.calls[2]?.[1]?.headers);
    expect(secondPatchHeaders.get("If-Match")).toBe('"etag-2"');
  });

  test("a 412's current ETag replaces the stale retained tag", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(withEtag({ status: null }, '"stale"'))
      .mockResolvedValueOnce(mismatchResponse('"current"'))
      .mockResolvedValueOnce(withEtag({ status: "read" }, '"current-after-retry"'));

    await apiFetch("/api/v1/books/book-1/reading");
    await expect(
      apiFetch("/api/v1/books/book-1/reading", {
        method: "PATCH",
        body: JSON.stringify({ status: "reading" }),
      }),
    ).rejects.toMatchObject({ status: 412 });

    await apiFetch("/api/v1/books/book-1/reading", {
      method: "PATCH",
      body: JSON.stringify({ status: "read" }),
    });

    const retryHeaders = new Headers(fetchSpy.mock.calls[2]?.[1]?.headers);
    expect(retryHeaders.get("If-Match")).toBe('"current"');
  });

  test("a resource with no retained tag PATCHes without If-Match", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ status: "reading" }));

    await apiFetch("/api/v1/books/never-read/reading", {
      method: "PATCH",
      body: JSON.stringify({ status: "reading" }),
    });

    const headers = new Headers(fetchSpy.mock.calls[0]?.[1]?.headers);
    expect(headers.has("If-Match")).toBe(false);
  });
});
