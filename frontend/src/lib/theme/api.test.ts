import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { patchTheme } from "./api";

// patchTheme must ride apiFetch's CSRF injection; the token module is
// stubbed so the wire assertion below is deterministic.
vi.mock("@/api/csrf", () => ({
  getCsrfToken: () => "test-csrf-token-value",
  refreshCsrfToken: vi.fn(),
  // The global test setup seeds the cache through this on every test.
  __seedCsrfTokenForTesting: vi.fn(),
  __resetCsrfTokenForTesting: vi.fn(),
}));

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

beforeEach(() => {
  vi.restoreAllMocks();
});

describe("patchTheme", () => {
  test("sends the X-CSRF-Token header on the wire", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(new Response(null, { status: 204 }));
    const result = await patchTheme("dark");
    expect(result.ok).toBe(true);
    const [url, init] = fetchSpy.mock.calls[0];
    expect(url).toBe("/auth/me/theme");
    expect(init?.method).toBe("PATCH");
    const headers = new Headers(init?.headers);
    expect(headers.get("X-CSRF-Token")).toBe("test-csrf-token-value");
    expect(init?.credentials).toBe("same-origin");
  });

  test("maps a CSRF rejection to ok: false with the response status", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      jsonResponse(428, {
        type: "https://reverie.example/probs/csrf-missing",
        title: "Precondition Required",
        status: 428,
      }),
    );
    const result = await patchTheme("dark");
    expect(result).toEqual({ ok: false, status: 428 });
  });

  test("maps other 4xx failures to ok: false with the status", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      jsonResponse(422, { title: "Unprocessable", status: 422 }),
    );
    const result = await patchTheme("light");
    expect(result).toEqual({ ok: false, status: 422 });
  });

  test("network failures still throw for the provider's try/catch", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new TypeError("network down"));
    await expect(patchTheme("system")).rejects.toThrow("network down");
  });
});
