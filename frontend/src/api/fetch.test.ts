import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __resetCsrfTokenForTesting } from "./csrf";
import { __resetEtagCacheForTesting } from "./etags";
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
    await seedCsrf(SAMPLE_TOKEN);
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

describe("apiFetch — lazy CSRF hydration", () => {
  test("first mutating call with an empty cache refreshes the token before the request", async () => {
    // An OIDC session never runs loginLocal (the only other hydration
    // site), so the wrapper must hydrate on first mutating use itself.
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ csrf_token: SAMPLE_TOKEN }))
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books", { method: "POST", body: "{}" });

    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(fetchSpy.mock.calls[0]?.[0]).toBe("/auth/me");
    const headers = new Headers(fetchSpy.mock.calls[1]?.[1]?.headers);
    expect(headers.get("X-CSRF-Token")).toBe(SAMPLE_TOKEN);
  });

  test("forwards the caller's abort signal to the lazy hydration request", async () => {
    // An aborted operation must cancel the pre-flight /auth/me too, not
    // leave it in flight to write the shared token cache post-abort.
    const controller = new AbortController();
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ csrf_token: SAMPLE_TOKEN }))
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books", {
      method: "POST",
      body: "{}",
      signal: controller.signal,
    });

    expect(fetchSpy.mock.calls[0]?.[0]).toBe("/auth/me");
    expect(fetchSpy.mock.calls[0]?.[1]?.signal).toBe(controller.signal);
  });

  test("GET with an empty cache never hydrates", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books");

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(fetchSpy.mock.calls[0]?.[0]).toBe("/api/v1/books");
  });

  test("a cached token skips hydration", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books", { method: "POST", body: "{}" });

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(fetchSpy.mock.calls[0]?.[0]).toBe("/api/v1/books");
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

  test("forwards the caller's abort signal to the retry refresh", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const controller = new AbortController();
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

    await apiFetch("/api/v1/books", {
      method: "POST",
      body: "{}",
      signal: controller.signal,
    });

    // call[1] is the mid-retry /auth/me refresh.
    expect(fetchSpy.mock.calls[1]?.[0]).toBe("/auth/me");
    expect(fetchSpy.mock.calls[1]?.[1]?.signal).toBe(controller.signal);
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
    await seedCsrf(SAMPLE_TOKEN);
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(new Response(null, { status: 204 }));

    const result = await apiFetch("/api/v1/books/1", { method: "DELETE" });
    expect(result).toBeUndefined();
  });

  test("205 Reset Content returns undefined without parsing", async () => {
    await seedCsrf(SAMPLE_TOKEN);
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

  test("a retained tag on a manifestation's metadata resource is not sent for an unrelated shelves PATCH", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(withEtag({ fields: {} }, '"meta-etag"'))
      .mockResolvedValueOnce(jsonResponse({ id: "shelf-1", name: "Renamed" }));

    await apiFetch("/api/v1/manifestations/book-1/metadata");
    await apiFetch("/api/v1/shelves/shelf-1", {
      method: "PATCH",
      body: JSON.stringify({ name: "Renamed" }),
    });

    const headers = new Headers(fetchSpy.mock.calls[1]?.[1]?.headers);
    expect(headers.has("If-Match")).toBe(false);
  });

  test("a GET on the metadata review-queue path and a PATCH on the books metadata path share a retained tag", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(withEtag([], '"meta-etag"'))
      .mockResolvedValueOnce(withEtag({ fields: {} }, '"meta-etag-2"'));

    await apiFetch("/api/v1/manifestations/book-1/metadata");
    await apiFetch("/api/v1/books/book-1/metadata", {
      method: "PATCH",
      body: JSON.stringify({ title: "New" }),
    });

    const headers = new Headers(fetchSpy.mock.calls[1]?.[1]?.headers);
    expect(headers.get("If-Match")).toBe('"meta-etag"');
  });
});
