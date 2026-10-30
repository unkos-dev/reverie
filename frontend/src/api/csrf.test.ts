import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { __resetCsrfTokenForTesting, getCsrfToken, refreshCsrfToken } from "./csrf";

const SAMPLE_TOKEN = "abcdefghijklmnopqrstuvwxyz0123456789_-ABCDE"; // 43 chars

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

describe("refreshCsrfToken", () => {
  test("clears cache when csrf_token is empty string", async () => {
    // Empty-string token would otherwise inject a blank `X-CSRF-Token`
    // header that the enforcing middleware rejects → guaranteed 403 on
    // every mutating request. Schema's `.min(1)` funnels this into the
    // null-cache path.
    vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ csrf_token: SAMPLE_TOKEN }))
      .mockResolvedValueOnce(jsonResponse({ csrf_token: "" }));
    await refreshCsrfToken();
    expect(getCsrfToken()).toBe(SAMPLE_TOKEN);

    expect(await refreshCsrfToken()).toBeNull();
    expect(getCsrfToken()).toBeNull();
  });
});
