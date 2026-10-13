import { describe, expect, test, vi } from "vite-plus/test";

import { getInputCounts, listInputs, scanIngestion } from "./ingestion";

vi.mock("./fetch", () => ({ apiFetch: vi.fn() }));
// eslint-disable-next-line import/first -- imported after the mock is registered
import { apiFetch } from "./fetch";

const ITEM = {
  id: "019a0000-0000-7000-8000-000000000001",
  path: "incoming/Marlow - The Salt Archive.epub",
  status: "rejected",
  outcome: "rejected",
  primary_reason: "damaged",
  reasons: ["damaged", "invalid_structure"],
  observed_at: "2026-10-09T08:12:00Z",
  completed_at: "2026-10-09T08:12:30Z",
};

describe("ingestion API client", () => {
  test("listInputs sends the reason, the page size and the cursor", async () => {
    vi.mocked(apiFetch).mockResolvedValue({ items: [ITEM], next_cursor: "abc" });
    const page = await listInputs({ reason: "damaged", cursor: "c1" });
    expect(page.items).toHaveLength(1);
    expect(page.next_cursor).toBe("abc");
    expect(vi.mocked(apiFetch).mock.calls.at(-1)?.[0]).toBe(
      "/api/v1/ingestion/inputs?reason=damaged&limit=25&cursor=c1",
    );
  });

  test("listInputs omits the cursor on the first page and accepts a null completion time", async () => {
    vi.mocked(apiFetch).mockResolvedValue({
      items: [{ ...ITEM, completed_at: null, outcome: null }],
      next_cursor: null,
    });
    const page = await listInputs({ reason: "needs_change" });
    expect(page.items[0]?.completed_at).toBeNull();
    expect(vi.mocked(apiFetch).mock.calls.at(-1)?.[0]).toBe(
      "/api/v1/ingestion/inputs?reason=needs_change&limit=25",
    );
  });

  test("listInputs rejects a reason class outside the closed set", async () => {
    vi.mocked(apiFetch).mockResolvedValue({
      items: [{ ...ITEM, primary_reason: "raw text from the validator" }],
    });
    await expect(listInputs({ reason: "damaged" })).rejects.toThrow();
  });

  test("listInputs rejects an offset timestamp", async () => {
    vi.mocked(apiFetch).mockResolvedValue({
      items: [{ ...ITEM, observed_at: "2026-10-09T16:12:00+08:00" }],
    });
    await expect(listInputs({ reason: "damaged" })).rejects.toThrow();
  });

  test("getInputCounts parses by_reason and the attention total", async () => {
    vi.mocked(apiFetch).mockResolvedValue({
      by_reason: [{ reason: "damaged", count: 2 }],
      attention_total: 2,
    });
    await expect(getInputCounts()).resolves.toEqual({
      by_reason: [{ reason: "damaged", count: 2 }],
      attention_total: 2,
    });
  });

  test("getInputCounts rejects a negative count", async () => {
    vi.mocked(apiFetch).mockResolvedValue({
      by_reason: [{ reason: "damaged", count: -1 }],
      attention_total: 0,
    });
    await expect(getInputCounts()).rejects.toThrow();
  });

  test("scanIngestion posts and parses the discovery counts", async () => {
    vi.mocked(apiFetch).mockResolvedValue({
      queued: 1,
      deferred: 2,
      suppressed: 3,
      monitor: "/api/v1/dashboard/activity",
    });
    await expect(scanIngestion()).resolves.toMatchObject({ queued: 1, deferred: 2, suppressed: 3 });
    expect(vi.mocked(apiFetch).mock.calls.at(-1)).toEqual([
      "/api/v1/ingestion/scan",
      { method: "POST" },
    ]);
  });
});
