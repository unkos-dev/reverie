import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { __resetCsrfTokenForTesting } from "./csrf";
import { searchLibrary } from "./search";

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

describe("searchLibrary", () => {
  test("calls GET /api/v1/search with the q param", async () => {
    const body = { items: [] };
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(body));

    await searchLibrary("galaxy");

    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.pathname).toBe("/api/v1/search");
    expect(url.searchParams.get("q")).toBe("galaxy");
  });

  test("forwards an AbortSignal when provided", async () => {
    const controller = new AbortController();
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ items: [] }));

    await searchLibrary("galaxy", controller.signal);

    const init = fetchSpy.mock.calls[0]?.[1];
    expect(init?.signal).toBe(controller.signal);
  });

  test("parses a well-formed book hit", async () => {
    const body = {
      items: [
        {
          kind: "book",
          id: "11111111-1111-1111-1111-111111111111",
          work_id: "22222222-2222-2222-2222-222222222222",
          title: "Stoner",
          authors: ["John Williams"],
          snippet: "the galaxy awaits",
          cover_url: "/api/v1/books/.../cover/thumb",
        },
      ],
    };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(body));

    const result = await searchLibrary("galaxy");

    expect(result.items).toHaveLength(1);
    expect(result.items[0]?.kind).toBe("book");
    expect(result.items[0]?.title).toBe("Stoner");
    expect(result.items[0]?.authors).toEqual(["John Williams"]);
  });
});

describe("response schema validation (zod boundary)", () => {
  test("rejects a response with missing items", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse({}));

    await expect(searchLibrary("x")).rejects.toThrow();
  });

  test("rejects an unknown kind variant", async () => {
    // `kind: "author"` is not yet in the enum. The schema must reject
    // it now (loud failure) rather than silently passing through with
    // an unknown discriminator.
    const body = {
      items: [
        {
          kind: "author",
          id: "x",
          work_id: null,
          title: "t",
          authors: [],
          snippet: null,
          cover_url: null,
        },
      ],
    };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(body));

    await expect(searchLibrary("x")).rejects.toThrow();
  });

  test("rejects a hit with wrong snippet type", async () => {
    const body = {
      items: [
        {
          kind: "book",
          id: "x",
          work_id: null,
          title: "t",
          authors: [],
          snippet: 12345,
          cover_url: null,
        },
      ],
    };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(body));

    await expect(searchLibrary("x")).rejects.toThrow();
  });
});
