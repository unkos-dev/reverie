import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";

import { loader } from "./library";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

import { RouterContextProvider, type LoaderFunctionArgs } from "react-router";

function loaderArgs(url: string): LoaderFunctionArgs {
  return {
    request: new Request(url),
    params: {},
    context: new RouterContextProvider(),
    url: new URL(url),
    pattern: "/library",
  };
}

beforeEach(() => {
  queryClient.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  queryClient.clear();
});

describe("library loader", () => {
  test("seeds the page-1 cache slot keyed without cursor", async () => {
    const body = { items: [], next_cursor: null };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(body));

    await loader(loaderArgs("http://localhost/library"));

    const seeded = queryClient.getQueryData(queryKeys.books.list({}));
    expect(seeded).toBeDefined();
  });

  test("strips the cursor query param from the cache key", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({ items: [], next_cursor: null }),
    );

    await loader(loaderArgs("http://localhost/library?cursor=abc123"));

    // Cache key is `["books", "list", {}]` — cursor stripped.
    expect(queryClient.getQueryData(queryKeys.books.list({}))).toBeDefined();
    // The cursor-bearing key is NOT used.
    expect(queryClient.getQueryData(queryKeys.books.list({ cursor: "abc123" }))).toBeUndefined();
  });

  test("preserves non-cursor params (sort, author, series, shelf, q) in the key", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({ items: [], next_cursor: null }),
    );

    await loader(loaderArgs("http://localhost/library?sort=title&author=a1"));

    expect(
      queryClient.getQueryData(queryKeys.books.list({ sort: "title", author: ["a1"] })),
    ).toBeDefined();
  });

  test("returns null on success (data lives in the cache, not the loader return)", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({ items: [], next_cursor: null }),
    );

    const result = await loader(loaderArgs("http://localhost/library"));
    expect(result).toBeNull();
  });
});
