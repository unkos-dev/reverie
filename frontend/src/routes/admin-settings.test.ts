import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __resetEtagCacheForTesting } from "@/api/etags";
import type { AuthMe } from "@/hooks/useAuthMe";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";

import { loader } from "./admin-settings";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json", ETag: '"rev-1"' },
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
    has_local_password: true,
    theme_preference: "system",
    csrf_token: null,
  };
}

const SETTINGS_BODY = {
  accepted_formats: ["epub"],
  cleanup_imported: false,
  cleanup_duplicates: false,
  enrichment_enabled: true,
  enrichment_concurrency: 4,
  enrichment_poll_idle_secs: 30,
  enrichment_fetch_budget_secs: 20,
  cover_max_bytes: 10485760,
  cover_download_timeout_secs: 15,
  cover_min_long_edge_px: 600,
  cover_redirect_limit: 3,
  writeback_enabled: true,
  writeback_concurrency: 2,
  writeback_poll_idle_secs: 30,
  writeback_max_attempts: 5,
  opds_enabled: true,
  opds_page_size: 50,
  revision: 1,
  updated_at: "2026-10-09T10:31:00Z",
  restart_required_fields: [],
};

beforeEach(() => {
  queryClient.clear();
  __resetEtagCacheForTesting();
  vi.restoreAllMocks();
});

afterEach(() => {
  queryClient.clear();
});

describe("admin settings loader", () => {
  test("seeds the settings snapshot when the cached identity is admin", async () => {
    queryClient.setQueryData(queryKeys.auth.me(), authMe("admin"));
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(SETTINGS_BODY));

    await loader();

    expect(queryClient.getQueryData(queryKeys.settings.snapshot())).toMatchObject({ revision: 1 });
  });

  test("fetches fresh on every entry even when a snapshot is already cached", async () => {
    queryClient.setQueryData(queryKeys.auth.me(), authMe("admin"));
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse(SETTINGS_BODY))
      .mockResolvedValueOnce(jsonResponse({ ...SETTINGS_BODY, revision: 2 }));

    await loader();
    await loader();

    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(queryClient.getQueryData(queryKeys.settings.snapshot())).toMatchObject({ revision: 2 });
  });

  test("resolves without throwing when the prefetch fails, leaving the cache cold", async () => {
    queryClient.setQueryData(queryKeys.auth.me(), authMe("admin"));
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response("upstream failure", { status: 500 }),
    );

    expect(await loader()).toBeNull();
    expect(queryClient.getQueryData(queryKeys.settings.snapshot())).toBeUndefined();
  });

  test("skips the request when the cached identity is not admin", async () => {
    queryClient.setQueryData(queryKeys.auth.me(), authMe("adult"));
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    await loader();

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  test("skips the request when the identity is not cached yet", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    await loader();

    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
