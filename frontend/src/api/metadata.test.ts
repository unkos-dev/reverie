import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "./csrf";
import { rejectVersion } from "./metadata";

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
