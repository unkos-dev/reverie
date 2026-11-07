import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { listBooks } from "@/api/books";

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

describe("listBooks", () => {
  test("omits undefined params from the URL", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ items: [], next_cursor: null }));

    await listBooks({ author: ["aaaa1111"] });

    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.searchParams.has("series")).toBe(false);
    expect(url.searchParams.has("shelf")).toBe(false);
    expect(url.searchParams.has("q")).toBe(false);
    expect(url.searchParams.has("sort")).toBe(false);
    expect(url.searchParams.has("cursor")).toBe(false);
  });
});
