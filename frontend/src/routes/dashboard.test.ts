import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import type { AuthMe } from "@/hooks/useAuthMe";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";

import { loader } from "./dashboard";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

function authMe(role: AuthMe["role"]): AuthMe {
  return {
    id: "11111111-1111-4111-8111-111111111111",
    display_name: "Admin",
    email: "admin@example.com",
    role,
    is_child: false,
    theme_preference: "system",
    csrf_token: null,
  };
}

function dashboardStats(): Record<string, unknown> {
  return {
    total_manifestations: 0,
    total_works: 0,
    storage_total_bytes: 0,
    storage_cover_bytes: 0,
    storage_by_format: [],
    validation_breakdown: [],
    clean_non_epub_count: 0,
    enrichment_breakdown: [],
    metadata_coverage: {
      total: 0,
      has_description: 0,
      has_language: 0,
      has_isbn_13: 0,
      has_cover: 0,
    },
  };
}

beforeEach(() => {
  queryClient.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  queryClient.clear();
});

describe("dashboard loader", () => {
  test("seeds the stats cache when the cached identity is admin", async () => {
    queryClient.setQueryData(queryKeys.auth.me(), authMe("admin"));
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(dashboardStats()));

    await loader();

    expect(queryClient.getQueryData(queryKeys.dashboard.stats())).toBeDefined();
  });

  test("resolves without throwing when the prefetch fails, leaving the cache cold", async () => {
    queryClient.setQueryData(queryKeys.auth.me(), authMe("admin"));
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response("upstream blew up", { status: 500 }),
    );

    const result = await loader();

    expect(result).toBeNull();
    expect(queryClient.getQueryData(queryKeys.dashboard.stats())).toBeUndefined();
  });

  test("skips the prefetch entirely when the cached identity is not admin", async () => {
    queryClient.setQueryData(queryKeys.auth.me(), authMe("adult"));
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    await loader();

    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
