import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "./csrf";
import { updateReadingState } from "./reading";

function parseJsonBody(body: BodyInit | null | undefined): unknown {
  if (typeof body !== "string") throw new Error("expected stringified JSON body");
  return JSON.parse(body);
}

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

const FULL_ROW = {
  status: "reading",
  rating: null,
  notes: null,
  progress_pct: null,
  started_at: "2026-07-01T00:00:00Z",
  finished_at: null,
  last_read_at: null,
};

beforeEach(() => {
  // Seed (not reset): apiFetch lazily hydrates an empty cache with a
  // leading /auth/me fetch that would eat these suites' response mocks;
  // hydration behaviour itself is pinned in fetch.test.ts.
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
  vi.restoreAllMocks();
});

describe("updateReadingState", () => {
  test("PATCHes /api/v1/books/{id}/reading with a bare merge-patch body", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(FULL_ROW));

    await updateReadingState("abc-123", { status: "reading" });

    const [input, init] = fetchSpy.mock.calls[0] ?? [];
    expect(input).toBe("/api/v1/books/abc-123/reading");
    expect(init?.method).toBe("PATCH");
    expect(parseJsonBody(init?.body)).toEqual({ status: "reading" });
  });

  test("returns the parsed post-merge row", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(FULL_ROW));

    const result = await updateReadingState("abc-123", { status: "reading" });

    expect(result).toEqual(FULL_ROW);
  });

  test("null clears a field in the request body", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({
        status: null,
        rating: null,
        notes: null,
        progress_pct: null,
        started_at: null,
        finished_at: null,
        last_read_at: null,
      }),
    );

    await updateReadingState("abc-123", { status: null, rating: null });

    expect(parseJsonBody(fetchSpy.mock.calls[0]?.[1]?.body)).toEqual({
      status: null,
      rating: null,
    });
  });

  test("throws when the response body does not match ReadingStateSchema", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({
        status: "not_a_real_status",
        rating: null,
        notes: null,
        progress_pct: null,
        started_at: null,
        finished_at: null,
        last_read_at: null,
      }),
    );

    await expect(updateReadingState("abc-123", { status: "reading" })).rejects.toThrow();
  });

  test("surfaces 422 validation as ApiError with the detail preserved", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          type: "https://reverie.example/probs/validation",
          title: "Unprocessable Entity",
          status: 422,
          detail: "rating must be between 1 and 5",
        }),
        {
          status: 422,
          headers: { "Content-Type": "application/problem+json" },
        },
      ),
    );

    await expect(updateReadingState("abc-123", { rating: 9 })).rejects.toMatchObject({
      status: 422,
      detail: "rating must be between 1 and 5",
    });
  });
});
