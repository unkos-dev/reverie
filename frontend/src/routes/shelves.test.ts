import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";

import { loader } from "./shelves";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

function shelvesPage(): Record<string, unknown> {
  return {
    items: [
      {
        id: "s-1",
        name: "Currently Reading",
        is_system: true,
        created_at: "2026-01-01T00:00:00Z",
        updated_at: "2026-01-01T00:00:00Z",
        item_count: 0,
      },
    ],
    next_cursor: null,
  };
}

beforeEach(() => {
  queryClient.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  queryClient.clear();
});

describe("shelves loader", () => {
  test("seeds the shelf list cache on success", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(shelvesPage()));

    await loader();

    expect(queryClient.getQueryData(queryKeys.shelves.list())).toBeDefined();
  });

  test("resolves without throwing when the prefetch fails, leaving the cache cold", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response("upstream blew up", { status: 500 }),
    );

    const result = await loader();

    expect(result).toBeNull();
    expect(queryClient.getQueryData(queryKeys.shelves.list())).toBeUndefined();
  });
});
