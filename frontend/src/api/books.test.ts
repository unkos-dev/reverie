import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __resetCsrfTokenForTesting } from "./csrf";
import { getBook, getWork, listBooks, updateBookMetadata } from "./books";
import { ApiError } from "./errors";

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

beforeEach(() => {
  __resetCsrfTokenForTesting();
  vi.restoreAllMocks();
});

afterEach(() => {
  __resetCsrfTokenForTesting();
});

describe("listBooks", () => {
  test("calls GET /api/v1/books and returns the envelope", async () => {
    const body = {
      items: [
        {
          id: "11111111-1111-1111-1111-111111111111",
          work_id: "22222222-2222-2222-2222-222222222222",
          title: "Stoner",
          subtitle: null,
          authors: ["John Williams"],
          series: null,
          isbn_13: "9781590171998",
          pages: 288,
          cover_url: "/api/v1/books/.../cover/thumb",
          ingestion_status: "complete",
          validation_status: "clean",
          enrichment_status: "complete",
          reading_state: { status: "finished", rating: 5, progress_pct: 100 },
          created_at: "2026-01-01T00:00:00Z",
        },
      ],
      next_cursor: null,
    };
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(body));

    const result = await listBooks();

    expect(result.items).toHaveLength(1);
    expect(result.next_cursor).toBeNull();
    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.pathname).toBe("/api/v1/books");
    expect(url.search).toBe("");
  });

  test("serialises filter params into the query string", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ items: [], next_cursor: null }));

    await listBooks({
      author: ["aaaa1111"],
      series: "bbbb2222",
      sort: "title",
      cursor: "eyJ4Ijox",
    });

    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.searchParams.get("author")).toBe("aaaa1111");
    expect(url.searchParams.get("series")).toBe("bbbb2222");
    expect(url.searchParams.get("sort")).toBe("title");
    expect(url.searchParams.get("cursor")).toBe("eyJ4Ijox");
  });

  test("omits undefined params from the URL", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ items: [], next_cursor: null }));

    await listBooks({ author: ["aaaa1111"] });

    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.searchParams.has("series")).toBe(false);
    expect(url.searchParams.has("shelf")).toBe(false);
    expect(url.searchParams.has("q")).toBe(false);
    expect(url.searchParams.has("sort")).toBe(false);
    expect(url.searchParams.has("cursor")).toBe(false);
  });

  test("forwards AbortSignal", async () => {
    const controller = new AbortController();
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ items: [], next_cursor: null }));

    await listBooks({}, controller.signal);

    expect(fetchSpy.mock.calls[0]?.[1]?.signal).toBe(controller.signal);
  });
});

describe("getBook", () => {
  test("returns 404-bearing ApiError when the manifestation is hidden", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          type: "https://reverie.example/probs/not-found",
          title: "Not Found",
          status: 404,
          detail: "Resource not found.",
        }),
        { status: 404, headers: { "Content-Type": "application/problem+json" } },
      ),
    );
    await expect(getBook("ghost")).rejects.toBeInstanceOf(ApiError);
  });

  test("calls GET /api/v1/books/{id} with the id percent-encoded", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({
        id: "abc",
        work_id: "w-1",
        title: "x",
        authors: [],
        series: null,
        description: null,
        language: null,
        isbn_13: null,
        isbn_10: null,
        publisher: null,
        pub_date: null,
        cover_url: "",
        tags: [],
        ingestion_status: "complete",
        validation_status: "clean",
        enrichment_status: "complete",
        metadata_version_summary: { pending: 0, accepted: 0 },
        metadata_versions: [],
        created_at: "2026-01-01T00:00:00Z",
        updated_at: "2026-01-01T00:00:00Z",
      }),
    );

    await getBook("abc/with-slash");

    const url = fetchSpy.mock.calls[0]?.[0] as string;
    expect(url).toBe("/api/v1/books/abc%2Fwith-slash");
  });
});

describe("getWork", () => {
  test("calls GET /api/v1/works/{id}", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({
        id: "w-1",
        title: "x",
        authors: [],
        description: null,
        language: null,
        series: null,
        manifestations: [],
      }),
    );

    await getWork("w-1");

    const url = fetchSpy.mock.calls[0]?.[0] as string;
    expect(url).toBe("/api/v1/works/w-1");
  });

  test("percent-encodes the id (defensive against malformed input)", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({
        id: "w-1",
        title: "x",
        authors: [],
        description: null,
        language: null,
        series: null,
        manifestations: [],
      }),
    );

    await getWork("work/with-slash");

    const url = fetchSpy.mock.calls[0]?.[0] as string;
    expect(url).toBe("/api/v1/works/work%2Fwith-slash");
  });
});

describe("response schema validation (zod boundary)", () => {
  test("listBooks throws when the response is missing required fields", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse({ items: [] }));
    // `next_cursor` omitted — schema requires it.
    await expect(listBooks()).rejects.toThrow();
  });

  test("listBooks throws when ingestion_status is not one of the known variants", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({
        items: [
          {
            id: "x",
            work_id: "w",
            title: "t",
            subtitle: null,
            authors: [],
            series: null,
            isbn_13: null,
            pages: null,
            reading_state: null,
            cover_url: "",
            ingestion_status: "bogus_state",
            validation_status: "clean",
            enrichment_status: "complete",
            created_at: "2026-01-01T00:00:00Z",
          },
        ],
        next_cursor: null,
      }),
    );
    await expect(listBooks()).rejects.toThrow();
  });

  test("listBooks throws when validation_status is the pre-migration 'valid' value", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({
        items: [
          {
            id: "x",
            work_id: "w",
            title: "t",
            subtitle: null,
            authors: [],
            series: null,
            isbn_13: null,
            pages: null,
            reading_state: null,
            cover_url: "",
            ingestion_status: "complete",
            validation_status: "valid",
            enrichment_status: "complete",
            created_at: "2026-01-01T00:00:00Z",
          },
        ],
        next_cursor: null,
      }),
    );
    await expect(listBooks()).rejects.toThrow();
  });

  test("getBook throws when description type is wrong (schema drift)", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({
        id: "abc",
        work_id: "w",
        title: "x",
        authors: [],
        series: null,
        description: 12345, // should be string | null
        language: null,
        isbn_13: null,
        isbn_10: null,
        publisher: null,
        pub_date: null,
        cover_url: "",
        tags: [],
        ingestion_status: "complete",
        validation_status: "clean",
        enrichment_status: "complete",
        metadata_version_summary: { pending: 0, accepted: 0 },
        metadata_versions: [],
        created_at: "2026-01-01T00:00:00Z",
        updated_at: "2026-01-01T00:00:00Z",
      }),
    );
    await expect(getBook("abc")).rejects.toThrow();
  });
});

describe("updateBookMetadata", () => {
  test("PATCHes /api/v1/books/{id}/metadata with a bare merge-patch body", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({
        fields: {
          title: {
            value: "New",
            version_id: "11111111-1111-4111-8111-111111111111",
            previous_version_id: null,
          },
        },
      }),
    );

    await updateBookMetadata("abc-123", { title: "New", description: null });

    const [input, init] = fetchSpy.mock.calls[0] ?? [];
    expect(input).toBe("/api/v1/books/abc-123/metadata");
    expect(init?.method).toBe("PATCH");
    expect(parseJsonBody(init?.body)).toEqual({ title: "New", description: null });
  });

  test("returns the parsed per-field version-change map", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({
        fields: {
          isbn_13: {
            value: "9781590171998",
            version_id: "22222222-2222-4222-8222-222222222222",
            previous_version_id: "33333333-3333-4333-8333-333333333333",
          },
        },
      }),
    );

    const result = await updateBookMetadata("abc-123", { isbn_13: "978-1-59017-199-8" });

    expect(result.fields.isbn_13).toEqual({
      value: "9781590171998",
      version_id: "22222222-2222-4222-8222-222222222222",
      previous_version_id: "33333333-3333-4333-8333-333333333333",
    });
  });

  test("throws when the response body does not match UpdateMetadataResponseSchema", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({
        fields: {
          title: { value: "New", version_id: "not-a-uuid", previous_version_id: null },
        },
      }),
    );

    await expect(updateBookMetadata("abc-123", { title: "New" })).rejects.toThrow();
  });

  test("surfaces 422 validation as ApiError with the title preserved", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          type: "https://reverie.example/probs/validation",
          title: "Unprocessable Entity",
          status: 422,
          detail: "no fields",
        }),
        {
          status: 422,
          headers: { "Content-Type": "application/problem+json" },
        },
      ),
    );

    await expect(updateBookMetadata("abc", {})).rejects.toMatchObject({
      status: 422,
      detail: "no fields",
    });
  });
});
