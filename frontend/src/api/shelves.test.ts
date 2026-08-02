import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "./csrf";
import {
  addShelfItem,
  buildEtag,
  createShelf,
  deleteShelf,
  getShelf,
  listShelves,
  removeShelfItem,
  renameShelf,
  reorderShelfItems,
} from "./shelves";

function parseJsonBody(body: BodyInit | null | undefined): unknown {
  if (typeof body !== "string") throw new Error("expected stringified JSON body");
  return JSON.parse(body);
}

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

function emptyResponse(status = 204): Response {
  return new Response(null, { status });
}

beforeEach(() => {
  // Seed (not reset): apiFetch lazily hydrates an empty cache with a
  // leading /auth/me fetch that would eat these suites' response mocks;
  // hydration behaviour itself is pinned in fetch.test.ts.
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
  vi.restoreAllMocks();
});

describe("buildEtag", () => {
  test("wraps the timestamp in double quotes", () => {
    expect(buildEtag("2026-05-24T03:00:00Z")).toBe('"2026-05-24T03:00:00Z"');
  });
});

function shelfRow(name: string, id: string) {
  return {
    id,
    name,
    is_system: false,
    created_at: "2026-05-24T01:00:00Z",
    updated_at: "2026-05-24T01:00:00Z",
    item_count: 3,
  };
}

describe("listShelves", () => {
  test("returns parsed items from a single page", async () => {
    const body = {
      items: [shelfRow("Currently reading", "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")],
      next_cursor: null,
    };
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(body));
    const result = await listShelves();
    expect(result).toHaveLength(1);
    expect(result[0]?.name).toBe("Currently reading");
    expect(result[0]?.item_count).toBe(3);
    const url = fetchSpy.mock.calls[0]?.[0] as string;
    expect(url).toBe("/api/v1/shelves");
  });

  test("walks next_cursor and assembles all pages", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(
        jsonResponse({
          items: [shelfRow("Page one", "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaa1")],
          next_cursor: "c2Vjb25k",
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          items: [shelfRow("Page two", "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaa2")],
          next_cursor: null,
        }),
      );
    const result = await listShelves();
    expect(result.map((s) => s.name)).toEqual(["Page one", "Page two"]);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    const secondUrl = fetchSpy.mock.calls[1]?.[0] as string;
    expect(secondUrl).toBe("/api/v1/shelves?cursor=c2Vjb25k");
  });
});

describe("getShelf", () => {
  test("calls GET /api/v1/shelves/{id}", async () => {
    const body = {
      id: "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
      name: "Holiday",
      is_system: false,
      created_at: "2026-05-24T01:00:00Z",
      updated_at: "2026-05-24T01:00:00Z",
      items: [],
      next_cursor: null,
    };
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(body));
    const result = await getShelf("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
    expect(result.name).toBe("Holiday");
    const url = fetchSpy.mock.calls[0]?.[0] as string;
    expect(url).toBe("/api/v1/shelves/bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
  });

  test("walks item pages and assembles the full list", async () => {
    const identity = {
      id: "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
      name: "Holiday",
      is_system: false,
      created_at: "2026-05-24T01:00:00Z",
      updated_at: "2026-05-24T01:00:00Z",
    };
    const item = (n: number) => ({
      manifestation_id: `00000000-0000-0000-0000-00000000000${String(n)}`,
      position: n,
      added_at: "2026-05-24T02:00:00Z",
    });
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(
        jsonResponse({ ...identity, items: [item(1), item(2)], next_cursor: "bmV4dA" }),
      )
      .mockResolvedValueOnce(jsonResponse({ ...identity, items: [item(3)], next_cursor: null }));
    const result = await getShelf("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
    expect(result.items.map((i) => i.position)).toEqual([1, 2, 3]);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    const secondUrl = fetchSpy.mock.calls[1]?.[0] as string;
    expect(secondUrl).toBe("/api/v1/shelves/bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb?cursor=bmV4dA");
  });

  test("pins identity (incl. updated_at) from the first page", async () => {
    const identity = {
      id: "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
      name: "Holiday",
      is_system: false,
      created_at: "2026-05-24T01:00:00Z",
    };
    vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(
        jsonResponse({
          ...identity,
          updated_at: "2026-05-24T01:00:00Z",
          items: [],
          next_cursor: "bmV4dA",
        }),
      )
      // A concurrent mutation bumped updated_at between pages — the
      // assembled result must keep page 1's ETag value.
      .mockResolvedValueOnce(
        jsonResponse({
          ...identity,
          updated_at: "2026-05-24T09:00:00Z",
          items: [],
          next_cursor: null,
        }),
      );
    const result = await getShelf("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
    expect(result.updated_at).toBe("2026-05-24T01:00:00Z");
  });

  test("throws loudly when pagination never terminates", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      Promise.resolve(
        jsonResponse({
          id: "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
          name: "Endless",
          is_system: false,
          created_at: "2026-05-24T01:00:00Z",
          updated_at: "2026-05-24T01:00:00Z",
          items: [],
          next_cursor: "bmV4dA",
        }),
      ),
    );
    await expect(getShelf("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb")).rejects.toThrow(
      "did not terminate",
    );
  });
});

describe("listShelves walk bound", () => {
  test("throws loudly when pagination never terminates", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      Promise.resolve(jsonResponse({ items: [], next_cursor: "bmV4dA" })),
    );
    await expect(listShelves()).rejects.toThrow("did not terminate");
  });
});

describe("createShelf", () => {
  test("POSTs name and parses response", async () => {
    const body = {
      id: "cccccccc-cccc-cccc-cccc-cccccccccccc",
      name: "Wishlist",
      is_system: false,
      created_at: "2026-05-24T01:00:00Z",
      updated_at: "2026-05-24T01:00:00Z",
      item_count: 0,
    };
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse(body, { status: 201 }));
    const result = await createShelf("Wishlist");
    expect(result.id).toBe("cccccccc-cccc-cccc-cccc-cccccccccccc");
    const url = fetchSpy.mock.calls[0]?.[0] as string;
    expect(url).toBe("/api/v1/shelves");
    const init = fetchSpy.mock.calls[0]?.[1];
    expect(init?.method).toBe("POST");
    expect(parseJsonBody(init?.body)).toEqual({ name: "Wishlist" });
  });
});

describe("renameShelf", () => {
  test("surfaces system-shelf-immutable as ApiError", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          type: "https://reverie.example/probs/system-shelf-immutable",
          title: "Conflict",
          status: 409,
          detail: "System shelves cannot be renamed or deleted.",
        }),
        {
          status: 409,
          headers: { "Content-Type": "application/problem+json" },
        },
      ),
    );
    await expect(renameShelf("dddddddd-dddd-dddd-dddd-dddddddddddd", "Mine")).rejects.toMatchObject(
      {
        status: 409,
        problemSlug: "system-shelf-immutable",
      },
    );
  });
});

describe("deleteShelf", () => {
  test("issues DELETE and resolves on 204", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(emptyResponse(204));
    await deleteShelf("eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee");
    const url = fetchSpy.mock.calls[0]?.[0] as string;
    expect(url).toBe("/api/v1/shelves/eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee");
    const init = fetchSpy.mock.calls[0]?.[1];
    expect(init?.method).toBe("DELETE");
  });
});

describe("shelf items", () => {
  test("addShelfItem POSTs manifestation_id", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(emptyResponse(204));
    await addShelfItem(
      "ffffffff-ffff-ffff-ffff-ffffffffffff",
      "00000000-0000-0000-0000-000000000001",
    );
    const init = fetchSpy.mock.calls[0]?.[1];
    expect(init?.method).toBe("POST");
    expect(parseJsonBody(init?.body)).toEqual({
      manifestation_id: "00000000-0000-0000-0000-000000000001",
    });
  });

  test("removeShelfItem issues DELETE on the nested path", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(emptyResponse(204));
    await removeShelfItem(
      "ffffffff-ffff-ffff-ffff-ffffffffffff",
      "00000000-0000-0000-0000-000000000002",
    );
    const url = fetchSpy.mock.calls[0]?.[0] as string;
    expect(url).toBe(
      "/api/v1/shelves/ffffffff-ffff-ffff-ffff-ffffffffffff/items/00000000-0000-0000-0000-000000000002",
    );
  });

  test("reorderShelfItems sends quoted If-Match header", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(emptyResponse(204));
    await reorderShelfItems(
      "ffffffff-ffff-ffff-ffff-ffffffffffff",
      ["00000000-0000-0000-0000-000000000003"],
      "2026-05-24T03:00:00Z",
    );
    const init = fetchSpy.mock.calls[0]?.[1];
    expect(init?.method).toBe("PUT");
    const headers = init?.headers as Headers | undefined;
    expect(headers?.get("If-Match")).toBe('"2026-05-24T03:00:00Z"');
  });

  test("reorderShelfItems surfaces 412 as ApiError", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          type: "https://reverie.example/probs/if-match-mismatch",
          title: "Precondition Failed",
          status: 412,
          detail: "Resource changed since last read.",
        }),
        {
          status: 412,
          headers: { "Content-Type": "application/problem+json" },
        },
      ),
    );
    await expect(
      reorderShelfItems("ffffffff-ffff-ffff-ffff-ffffffffffff", [], "stale"),
    ).rejects.toMatchObject({
      status: 412,
      problemSlug: "if-match-mismatch",
    });
  });
});
