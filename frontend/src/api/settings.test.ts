import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __resetEtagCacheForTesting } from "./etags";
import { ApiError, isIfMatchMismatch } from "./errors";
import { getReloadStatus, getSettings, updateSettings } from "./settings";

const VALUES = {
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
    headers: { "Content-Type": "application/json", ...headers },
  });
}

beforeEach(() => {
  __resetEtagCacheForTesting();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("getSettings", () => {
  test("splits the flattened body into values and row metadata", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      json({ ...GET_BODY, restart_required_fields: ["cover_max_bytes"] }),
    );

    const snapshot = await getSettings();

    expect(snapshot.revision).toBe(7);
    expect(snapshot.updatedAt).toBe("2026-10-09T10:31:00Z");
    expect(snapshot.restartRequiredFields).toEqual(["cover_max_bytes"]);
    expect(snapshot.values.enrichment_concurrency).toBe(4);
    expect(snapshot.values).not.toHaveProperty("revision");
  });

  test("rejects a timestamp that is not Z-terminated RFC 3339", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      json({ ...GET_BODY, updated_at: "2026-10-09T10:31:00+00:00" }),
    );
    await expect(getSettings()).rejects.toThrow();
  });

  test("rejects a cover size beyond the exact integer range instead of rounding it", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      json({ ...GET_BODY, cover_max_bytes: 2 ** 60 }),
    );
    await expect(getSettings()).rejects.toThrow();
  });

  test("rejects an unknown format", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      json({ ...GET_BODY, accepted_formats: ["docx"] }),
    );
    await expect(getSettings()).rejects.toThrow();
  });

  test("surfaces a 403 as an ApiError", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      json(
        { title: "Forbidden", status: 403 },
        { "Content-Type": "application/problem+json" },
        403,
      ),
    );
    await expect(getSettings()).rejects.toBeInstanceOf(ApiError);
  });
});

describe("getReloadStatus", () => {
  test("returns the reload timestamp", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      json({ last_successful_reload_at: "2026-10-09T10:42:00Z" }),
    );
    expect(await getReloadStatus()).toBe("2026-10-09T10:42:00Z");
  });

  test("returns null before the first reload", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(json({ last_successful_reload_at: null }));
    expect(await getReloadStatus()).toBeNull();
  });

  test("rejects a malformed timestamp", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      json({ last_successful_reload_at: "yesterday" }),
    );
    await expect(getReloadStatus()).rejects.toThrow();
  });

  test("does not disturb the retained settings tag", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(json(GET_BODY, { ETag: '"rev-7"' }))
      .mockResolvedValueOnce(json({ last_successful_reload_at: null }, { ETag: '"other"' }))
      .mockResolvedValueOnce(
        json({
          ...VALUES,
          revision: 8,
          updated_at: "2026-10-09T10:50:00Z",
          restart_required: false,
        }),
      );

    await getSettings();
    await getReloadStatus();
    await updateSettings({ opds_page_size: 25 });

    expect(new Headers(fetchSpy.mock.calls[2]?.[1]?.headers).get("If-Match")).toBe('"rev-7"');
  });
});

describe("updateSettings", () => {
  test("sends the patch with the tag the GET returned as If-Match", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(json(GET_BODY, { ETag: '"rev-7"' }))
      .mockResolvedValueOnce(
        json(
          {
            ...VALUES,
            opds_page_size: 25,
            revision: 8,
            updated_at: "2026-10-09T10:50:00Z",
            restart_required: false,
          },
          { ETag: '"rev-8"' },
        ),
      );

    await getSettings();
    const result = await updateSettings({ opds_page_size: 25 });

    const [url, init] = fetchSpy.mock.calls[1] ?? [];
    expect(url).toBe("/api/v1/settings");
    expect(init?.method).toBe("PUT");
    expect(init?.body).toBe('{"opds_page_size":25}');
    expect(new Headers(init?.headers).get("If-Match")).toBe('"rev-7"');
    expect(result.values.opds_page_size).toBe(25);
    expect(result.revision).toBe(8);
    expect(result.restartRequired).toBe(false);
  });

  test("a second write carries the tag the first write returned", async () => {
    const written = {
      ...VALUES,
      revision: 8,
      updated_at: "2026-10-09T10:50:00Z",
      restart_required: false,
    };
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(json(GET_BODY, { ETag: '"rev-7"' }))
      .mockResolvedValueOnce(json(written, { ETag: '"rev-8"' }))
      .mockResolvedValueOnce(json({ ...written, revision: 9 }, { ETag: '"rev-9"' }));

    await getSettings();
    await updateSettings({ opds_page_size: 25 });
    await updateSettings({ opds_page_size: 30 });

    const headers = new Headers(fetchSpy.mock.calls[2]?.[1]?.headers);
    expect(headers.get("If-Match")).toBe('"rev-8"');
  });

  test("a 412 is an ApiError with the if-match-mismatch slug", async () => {
    vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(json(GET_BODY, { ETag: '"rev-7"' }))
      .mockResolvedValueOnce(
        json(
          {
            type: "https://reverie.example/probs/if-match-mismatch",
            title: "Precondition Failed",
            status: 412,
          },
          { "Content-Type": "application/problem+json", ETag: '"rev-9"' },
          412,
        ),
      );

    await getSettings();
    const err: unknown = await updateSettings({ opds_page_size: 25 }).catch((e: unknown) => e);

    expect(isIfMatchMismatch(err)).toBe(true);
  });
});
