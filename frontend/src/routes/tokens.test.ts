import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";

import { loader } from "./tokens";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

function tokenList(): unknown[] {
  return [
    {
      id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
      name: "My Kindle",
      scopes: ["read"],
      expires_at: null,
      last_used_at: null,
      created_at: "2026-05-25T00:00:00Z",
    },
  ];
}

beforeEach(() => {
  queryClient.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  queryClient.clear();
});

describe("tokens loader", () => {
  test("seeds the token list cache on success", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(tokenList()));

    await loader();

    expect(queryClient.getQueryData(queryKeys.tokens.list())).toBeDefined();
  });

  test("resolves without throwing when the prefetch fails, leaving the cache cold", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response("upstream blew up", { status: 500 }),
    );

    const result = await loader();

    expect(result).toBeNull();
    expect(queryClient.getQueryData(queryKeys.tokens.list())).toBeUndefined();
  });
});
