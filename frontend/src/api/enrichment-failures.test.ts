import { describe, expect, test, vi } from "vite-plus/test";

import {
  getEnrichmentFailureCounts,
  listEnrichmentFailures,
  triggerEnrichment,
} from "./enrichment-failures";

vi.mock("./fetch", () => ({ apiFetch: vi.fn() }));
// eslint-disable-next-line import/first -- imported after the mock is registered
import { apiFetch } from "./fetch";

const ITEM = {
  manifestation_id: "019a0000-0000-7000-8000-0000000000a1",
  work_id: "019a0000-0000-7000-8000-0000000000b1",
  title: "The Salt Archive",
  status: "failed",
  attempt_count: 3,
  attempted_at: "2026-10-12T08:30:00Z",
  primary: { source: "openlibrary", class: "timeout" },
  also: [{ source: null, class: "internal" }],
};

describe("enrichment failures API client", () => {
  test("listEnrichmentFailures sends source and class for a sourced group", async () => {
    vi.mocked(apiFetch).mockResolvedValue({ items: [ITEM], next_cursor: null });
    const page = await listEnrichmentFailures({ source: "openlibrary", class: "timeout" });
    expect(page.items[0]?.primary.source).toBe("openlibrary");
    expect(vi.mocked(apiFetch).mock.calls.at(-1)?.[0]).toBe(
      "/api/v1/dashboard/enrichment-failures?class=timeout&source=openlibrary&limit=25",
    );
  });

  test("listEnrichmentFailures sends source=none for the group that belongs to no source", async () => {
    vi.mocked(apiFetch).mockResolvedValue({
      items: [{ ...ITEM, primary: { source: null, class: "internal" }, also: [] }],
    });
    await listEnrichmentFailures({ source: null, class: "internal", cursor: "next" });
    expect(vi.mocked(apiFetch).mock.calls.at(-1)?.[0]).toBe(
      "/api/v1/dashboard/enrichment-failures?class=internal&source=none&limit=25&cursor=next",
    );
  });

  test("a legacy unsourced book and a sourced book of one class each list only their own", async () => {
    const legacy = { ...ITEM, primary: { source: null, class: "unspecified" }, also: [] };
    const sourced = {
      ...ITEM,
      manifestation_id: "019a0000-0000-7000-8000-0000000000a2",
      primary: { source: "hardcover", class: "unspecified" },
      also: [],
    };
    vi.mocked(apiFetch).mockImplementation((url) => {
      const source = new URL(typeof url === "string" ? url : "", "http://x").searchParams.get(
        "source",
      );
      const items = [legacy, sourced].filter(
        (item) =>
          source === null ||
          (source === "none" ? item.primary.source === null : item.primary.source === source),
      );
      return Promise.resolve({ items, next_cursor: null });
    });
    const unsourced = await listEnrichmentFailures({ source: null, class: "unspecified" });
    const hardcover = await listEnrichmentFailures({ source: "hardcover", class: "unspecified" });
    expect(unsourced.items.map((i) => i.manifestation_id)).toEqual([legacy.manifestation_id]);
    expect(hardcover.items.map((i) => i.manifestation_id)).toEqual([sourced.manifestation_id]);
  });

  test("listEnrichmentFailures rejects a class outside the closed set", async () => {
    vi.mocked(apiFetch).mockResolvedValue({
      items: [{ ...ITEM, primary: { source: "openlibrary", class: "ECONNRESET at 10.0.0.4" } }],
    });
    await expect(
      listEnrichmentFailures({ source: "openlibrary", class: "timeout" }),
    ).rejects.toThrow();
  });

  test("getEnrichmentFailureCounts parses groups with and without a source", async () => {
    vi.mocked(apiFetch).mockResolvedValue({
      by_failure: [
        { source: "hardcover", class: "not_found", count: 1 },
        { source: null, class: "unspecified", count: 2 },
      ],
      total: 3,
    });
    const counts = await getEnrichmentFailureCounts();
    expect(counts.total).toBe(3);
    expect(counts.by_failure).toHaveLength(2);
  });

  test("triggerEnrichment posts to the book's trigger endpoint", async () => {
    vi.mocked(apiFetch).mockResolvedValue(undefined);
    await triggerEnrichment("019a0000-0000-7000-8000-0000000000a1");
    expect(vi.mocked(apiFetch).mock.calls.at(-1)).toEqual([
      "/api/v1/manifestations/019a0000-0000-7000-8000-0000000000a1/enrichment/trigger",
      { method: "POST" },
    ]);
  });
});
