import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { logout } from "./auth";
import { __resetCsrfTokenForTesting } from "./csrf";
import { ApiError } from "./errors";

beforeEach(() => {
  __resetCsrfTokenForTesting();
  vi.restoreAllMocks();
});

afterEach(() => {
  __resetCsrfTokenForTesting();
});

describe("logout", () => {
  test("sends POST /auth/logout with same-origin credentials", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response(null, { status: 204 }));

    await logout();

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const [input, init] = fetchSpy.mock.calls[0] ?? [];
    expect(input).toBe("/auth/logout");
    expect(init?.method).toBe("POST");
    expect(init?.credentials).toBe("same-origin");
  });

  test("resolves on an empty 204 response", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(new Response(null, { status: 204 }));

    await expect(logout()).resolves.toBeUndefined();
  });

  test("throws ApiError on a non-2xx response", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response("session backend unavailable", { status: 500 }),
    );

    await expect(logout()).rejects.toBeInstanceOf(ApiError);
  });

  test("propagates network failure to the caller", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValueOnce(new TypeError("Failed to fetch"));

    await expect(logout()).rejects.toThrow("Failed to fetch");
  });
});
