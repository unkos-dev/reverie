import { beforeEach, expect, test, vi } from "vite-plus/test";

import { patchTheme } from "@/lib/theme/api";

import { __seedCsrfTokenForTesting } from "@/api/csrf";

beforeEach(() => {
  vi.restoreAllMocks();
  __seedCsrfTokenForTesting("test-csrf-token-value");
});

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
