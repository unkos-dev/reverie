import { RouterContextProvider, type LoaderFunctionArgs } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";

import { loader } from "./shelf-detail";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

function shelfDetailPage(id: string): Record<string, unknown> {
  return {
    id,
    name: "Currently Reading",
    is_system: true,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    items: [],
    next_cursor: null,
  };
}

function args(id: string): LoaderFunctionArgs {
  const url = `http://localhost/shelves/${id}`;
  return {
    request: new Request(url),
    params: { id },
    context: new RouterContextProvider(),
    url: new URL(url),
    pattern: "/shelves/:id",
  };
}

beforeEach(() => {
  queryClient.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  queryClient.clear();
});

describe("shelf-detail loader", () => {
  test("seeds the shelf detail cache slot keyed by id on success", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(shelfDetailPage("shelf-1")));

    await loader(args("shelf-1"));

    expect(queryClient.getQueryData(queryKeys.shelves.detail("shelf-1"))).toBeDefined();
  });

  test("resolves without throwing when the prefetch fails, leaving the cache cold", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response("upstream blew up", { status: 500 }),
    );

    const result = await loader(args("shelf-1"));

    expect(result).toBeNull();
    expect(queryClient.getQueryData(queryKeys.shelves.detail("shelf-1"))).toBeUndefined();
  });
});
