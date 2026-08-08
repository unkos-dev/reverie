import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import { forgetActiveUser, rememberActiveUser } from "@/lib/active-user";
import { displayStorageKey } from "@/pages/library/display-storage";

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

  test("preserves non-cursor params (author, series, shelf, q) in the key", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({ items: [], next_cursor: null }),
    );

    await loader(loaderArgs("http://localhost/library?author=a1"));

    expect(queryClient.getQueryData(queryKeys.books.list({ author: ["a1"] }))).toBeDefined();
  });

  test("ignores a stale ?sort= param: sort has no URL form", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({ items: [], next_cursor: null }),
    );

    await loader(loaderArgs("http://localhost/library?sort=title"));

    expect(queryClient.getQueryData(queryKeys.books.list({}))).toBeDefined();
    expect(queryClient.getQueryData(queryKeys.books.list({ sort: "title" }))).toBeUndefined();
  });

  test("seeds the key with the mirrored sort override, normalized through the codec", async () => {
    // The component derives its first-render key from the same mirror, so
    // seeding from it is what keeps the loader's prefetch a hit for a
    // reader whose sort override followed them onto this device.
    rememberActiveUser("user-a");
    localStorage.setItem(
      displayStorageKey("user-a"),
      JSON.stringify({ density: null, hiddenColumns: null, view: null, sortStack: "-pages" }),
    );
    try {
      vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
        jsonResponse({ items: [], next_cursor: null }),
      );

      await loader(loaderArgs("http://localhost/library"));

      expect(queryClient.getQueryData(queryKeys.books.list({ sort: "-pages" }))).toBeDefined();
    } finally {
      localStorage.clear();
    }
  });

  test("a mirror left by a signed-out account never seeds the key", async () => {
    // The mirror survives sign-out for its owner's return, but with no
    // confirmed account it must not shape anyone's first request.
    rememberActiveUser("user-a");
    localStorage.setItem(
      displayStorageKey("user-a"),
      JSON.stringify({ density: null, hiddenColumns: null, view: null, sortStack: "-pages" }),
    );
    forgetActiveUser();
    try {
      vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
        jsonResponse({ items: [], next_cursor: null }),
      );

      await loader(loaderArgs("http://localhost/library"));

      expect(queryClient.getQueryData(queryKeys.books.list({}))).toBeDefined();
      expect(queryClient.getQueryData(queryKeys.books.list({ sort: "-pages" }))).toBeUndefined();
    } finally {
      localStorage.clear();
    }
  });

  test("a malformed mirrored sort degrades to the bare key instead of forking it", async () => {
    rememberActiveUser("user-a");
    localStorage.setItem(
      displayStorageKey("user-a"),
      JSON.stringify({ density: null, hiddenColumns: null, view: null, sortStack: "bogus,," }),
    );
    try {
      vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
        jsonResponse({ items: [], next_cursor: null }),
      );

      await loader(loaderArgs("http://localhost/library"));

      expect(queryClient.getQueryData(queryKeys.books.list({}))).toBeDefined();
    } finally {
      localStorage.clear();
    }
  });

  test("returns null on success (data lives in the cache, not the loader return)", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({ items: [], next_cursor: null }),
    );

    const result = await loader(loaderArgs("http://localhost/library"));
    expect(result).toBeNull();
  });
});
