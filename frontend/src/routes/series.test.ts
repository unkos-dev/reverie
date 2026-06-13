import type { LoaderFunctionArgs } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";

import { loader } from "./series";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

function seriesBody(id: string): Record<string, unknown> {
  return {
    id,
    name: "Series",
    sort_name: "series",
    works: [],
  };
}

function args(id: string | undefined): LoaderFunctionArgs {
  const url = "http://localhost/series/x";
  return {
    request: new Request(url),
    params: id === undefined ? {} : { id },
    context: undefined,
    url: new URL(url),
    pattern: "/series/:id",
  };
}

beforeEach(() => {
  queryClient.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  queryClient.clear();
});

describe("series loader", () => {
  test("seeds the series detail cache slot keyed by id", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(seriesBody("abc-series")));

    await loader(args("abc-series"));

    expect(queryClient.getQueryData(queryKeys.series.detail("abc-series"))).toBeDefined();
  });

  test("returns the series name as crumb title on success", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(seriesBody("abc-series")));
    const result = await loader(args("abc-series"));
    expect(result).toEqual({ title: "Series" });
  });

  test("returns null when the prefetch failed and the cache is cold", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response("upstream blew up", { status: 500 }),
    );
    const result = await loader(args("abc-series"));
    expect(result).toBeNull();
  });

  test("throws a 404 Response on a missing id without hitting the API", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    await expect(loader(args(undefined))).rejects.toSatisfy(
      (thrown: unknown) => thrown instanceof Response && thrown.status === 404,
    );
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
