import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __resetCsrfTokenForTesting } from "./csrf";
import { MAX_RESOLVE_IDS, resolveAuthors, suggestVocab } from "./suggest";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

const AUTHOR_A = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const AUTHOR_B = "bbbbbbbb-bbbb-4bbb-9bbb-bbbbbbbbbbbb";

beforeEach(() => {
  __resetCsrfTokenForTesting();
  vi.restoreAllMocks();
});

afterEach(() => {
  __resetCsrfTokenForTesting();
});

describe("suggestVocab", () => {
  test("calls GET /api/v1/{kind}/suggest with the q param", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ suggestions: [] }));

    await suggestVocab("genres", "fant");

    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.pathname).toBe("/api/v1/genres/suggest");
    expect(url.searchParams.get("q")).toBe("fant");
  });

  test("unwraps the suggestions envelope", async () => {
    const body = { suggestions: [{ id: AUTHOR_A, value: "Fantasy" }] };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(body));

    const rows = await suggestVocab("tags", "fan");

    expect(rows).toHaveLength(1);
    expect(rows[0]?.value).toBe("Fantasy");
  });

  test("forwards an AbortSignal when provided", async () => {
    const controller = new AbortController();
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ suggestions: [] }));

    await suggestVocab("authors", "leg", controller.signal);

    expect(fetchSpy.mock.calls[0]?.[1]?.signal).toBe(controller.signal);
  });

  test("rejects a response missing the suggestions envelope", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse({}));

    await expect(suggestVocab("moods", "x")).rejects.toThrow();
  });
});

describe("resolveAuthors", () => {
  test("short-circuits an empty id list without a request", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    const rows = await resolveAuthors([]);

    expect(rows).toEqual([]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  test("de-duplicates ids and repeats the id param", async () => {
    const body = {
      items: [
        { id: AUTHOR_A, value: "Ursula K. Le Guin" },
        { id: AUTHOR_B, value: "N. K. Jemisin" },
      ],
    };
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(body));

    const rows = await resolveAuthors([AUTHOR_A, AUTHOR_B, AUTHOR_A]);

    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.pathname).toBe("/api/v1/authors");
    expect(url.searchParams.getAll("id")).toEqual([AUTHOR_A, AUTHOR_B]);
    expect(rows).toHaveLength(2);
  });

  test("rejects a response missing the items envelope", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse({ suggestions: [] }));

    await expect(resolveAuthors([AUTHOR_A])).rejects.toThrow();
  });

  test("resolves more than 20 ids in a single request without truncating", async () => {
    // A filter URL carries up to 60 author ids (20 each across all/any/none);
    // every chip must resolve, so none may be dropped before the request.
    const ids = Array.from(
      { length: 25 },
      (_, i) => `00000000-0000-4000-8000-${String(i).padStart(12, "0")}`,
    );
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ items: [] }));

    await resolveAuthors(ids);

    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.searchParams.getAll("id")).toHaveLength(25);
  });

  test("clamps to the resolve cap", async () => {
    const ids = Array.from(
      { length: MAX_RESOLVE_IDS + 1 },
      (_, i) => `00000000-0000-4000-8000-${String(i).padStart(12, "0")}`,
    );
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ items: [] }));

    await resolveAuthors(ids);

    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.searchParams.getAll("id")).toHaveLength(MAX_RESOLVE_IDS);
  });
});
