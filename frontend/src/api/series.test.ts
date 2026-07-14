import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "./csrf";
import { ApiError } from "./errors";
import { getSeries } from "./series";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

beforeEach(() => {
  // Seed (not reset): apiFetch lazily hydrates an empty cache with a
  // leading /auth/me fetch that would eat these suites' response mocks;
  // hydration behaviour itself is pinned in fetch.test.ts.
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
  vi.restoreAllMocks();
});

describe("getSeries", () => {
  test("calls GET /api/v1/series/{id} and parses the body", async () => {
    const body = {
      id: "33333333-3333-3333-3333-333333333333",
      name: "Mistborn",
      sort_name: "mistborn",
      works: [
        {
          id: "44444444-4444-4444-4444-444444444444",
          title: "The Final Empire",
          authors: ["Brandon Sanderson"],
          position: 1.0,
          manifestations: [
            {
              id: "55555555-5555-5555-5555-555555555555",
              isbn_13: null,
              isbn_10: null,
              cover_url: "/api/v1/books/.../cover/thumb",
              ingestion_status: "complete",
              validation_status: "clean",
              enrichment_status: "complete",
              created_at: "2026-05-23T01:00:00Z",
            },
          ],
        },
        {
          id: "66666666-6666-6666-6666-666666666666",
          title: "The Well of Ascension",
          authors: ["Brandon Sanderson"],
          position: 2.0,
          manifestations: [],
        },
      ],
    };
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(body));

    const result = await getSeries("33333333-3333-3333-3333-333333333333");

    expect(result.name).toBe("Mistborn");
    expect(result.works).toHaveLength(2);
    expect(result.works[1]?.manifestations).toEqual([]);
    const url = fetchSpy.mock.calls[0]?.[0] as string;
    expect(url).toBe("/api/v1/series/33333333-3333-3333-3333-333333333333");
  });

  test("surfaces 404 as ApiError", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          type: "https://reverie.example/probs/not-found",
          title: "Not Found",
          status: 404,
          detail: "Resource not found.",
        }),
        {
          status: 404,
          headers: { "Content-Type": "application/problem+json" },
        },
      ),
    );

    await expect(getSeries("00000000-0000-0000-0000-000000000000")).rejects.toBeInstanceOf(
      ApiError,
    );
  });
});
