import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __resetEtagCacheForTesting } from "@/api/etags";
import { getSettings, updateSettings } from "@/api/settings";

const VALUES = {
  ingestion_enabled: true,
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
  provider_visibility: {},
};

const GET_BODY = {
  ...VALUES,
  revision: 7,
  updated_at: "2026-10-09T10:31:00Z",
  restart_required_fields: [],
};

function json(body: unknown, headers: Record<string, string> = {}, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json", ETag: '"rev-7"', ...headers },
  });
}

beforeEach(() => {
  __resetEtagCacheForTesting();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("updateSettings", () => {
  test("a later read cannot change the tag held with an earlier snapshot", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(json(GET_BODY, { ETag: '"rev-7"' }))
      .mockResolvedValueOnce(json({ ...GET_BODY, revision: 8 }, { ETag: '"rev-8"' }))
      .mockResolvedValueOnce(
        json(
          {
            ...VALUES,
            revision: 9,
            updated_at: GET_BODY.updated_at,
            restart_required: false,
          },
          { ETag: '"rev-9"' },
        ),
      );
    const held = await getSettings();
    await getSettings();
    await updateSettings({ opds_page_size: 25 }, held.etag);
    expect(new Headers(fetchSpy.mock.calls[2]?.[1]?.headers).get("If-Match")).toBe('"rev-7"');
  });
});
