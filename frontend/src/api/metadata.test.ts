/**
 * Smoke tests for the per-version metadata mutators. Each helper is a
 * thin wrapper around `apiFetch`, so we assert URL + method + body
 * shape rather than re-testing the underlying transport.
 */
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "./csrf";
import { acceptVersion, rejectVersion, revertField } from "./metadata";

function parseJsonBody(body: BodyInit | null | undefined): unknown {
  if (typeof body !== "string") throw new Error("expected stringified JSON body");
  return JSON.parse(body);
}

beforeEach(() => {
  // Seed (not reset): apiFetch lazily hydrates an empty cache with a
  // leading /auth/me fetch that would eat these suites' response mocks;
  // hydration behaviour itself is pinned in fetch.test.ts.
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
  vi.restoreAllMocks();
});

describe("acceptVersion", () => {
  test("POSTs the version id to the manifestation's accept endpoint", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response(null, { status: 200 }));

    await acceptVersion("m-1", "v-1");

    const [input, init] = fetchSpy.mock.calls[0] ?? [];
    expect(input).toBe("/api/v1/manifestations/m-1/metadata/accept");
    expect(init?.method).toBe("POST");
    expect(parseJsonBody(init?.body)).toEqual({ version_id: "v-1" });
  });
});

describe("rejectVersion", () => {
  test("POSTs the version id to the reject endpoint", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response(null, { status: 200 }));

    await rejectVersion("m-1", "v-9");

    expect(fetchSpy.mock.calls[0]?.[0]).toBe("/api/v1/manifestations/m-1/metadata/reject");
    expect(parseJsonBody(fetchSpy.mock.calls[0]?.[1]?.body)).toEqual({
      version_id: "v-9",
    });
  });
});

describe("revertField", () => {
  test("POSTs field_name + null version_id to clear", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response(null, { status: 200 }));

    await revertField("m-1", "description", null);

    expect(fetchSpy.mock.calls[0]?.[0]).toBe("/api/v1/manifestations/m-1/metadata/revert");
    expect(parseJsonBody(fetchSpy.mock.calls[0]?.[1]?.body)).toEqual({
      field_name: "description",
      version_id: null,
    });
  });

  test("POSTs a specific version_id to re-promote", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response(null, { status: 200 }));

    await revertField("m-1", "title", "v-7");

    expect(parseJsonBody(fetchSpy.mock.calls[0]?.[1]?.body)).toEqual({
      field_name: "title",
      version_id: "v-7",
    });
  });
});
