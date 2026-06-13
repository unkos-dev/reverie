import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";

import { loader } from "./book";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

function bookDetail(id: string): Record<string, unknown> {
  return {
    id,
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
  };
}

import type { LoaderFunctionArgs } from "react-router";

function args(id: string | undefined): LoaderFunctionArgs {
  const url = "http://localhost/b/x";
  return {
    request: new Request(url),
    params: id === undefined ? {} : { id },
    context: undefined,
    url: new URL(url),
    pattern: "/b/:id",
  };
}

beforeEach(() => {
  queryClient.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  queryClient.clear();
});

describe("book loader", () => {
  test("seeds the detail cache slot keyed by id", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(bookDetail("abc-123")));

    await loader(args("abc-123"));

    expect(queryClient.getQueryData(queryKeys.books.detail("abc-123"))).toBeDefined();
  });

  test("throws a 404 Response when the id param is missing", async () => {
    const result = await loader(args(undefined)).catch((e: unknown) => e);
    expect(result).toBeInstanceOf(Response);
    expect((result as Response).status).toBe(404);
  });

  test("throws a 404 Response when the id param is empty", async () => {
    const result = await loader(args("")).catch((e: unknown) => e);
    expect(result).toBeInstanceOf(Response);
    expect((result as Response).status).toBe(404);
  });

  test("returns the title for the breadcrumb on success", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(bookDetail("abc")));
    const result = await loader(args("abc"));
    expect(result).toEqual({ title: "x" });
  });

  test("returns null when the prefetch failed and the cache is cold", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response("upstream blew up", { status: 500 }),
    );
    const result = await loader(args("abc"));
    expect(result).toBeNull();
  });
});
