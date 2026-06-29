import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import {
  fetchSetupStatus,
  loginLocal,
  logout,
  requestPasswordReset,
  resetPassword,
  setupAdmin,
} from "./auth";
import { __resetCsrfTokenForTesting } from "./csrf";
import { ApiError } from "./errors";

/** Parse the JSON body passed to a mocked `fetch` call. */
function bodyOf(init: RequestInit | undefined): unknown {
  const raw = init?.body;
  return typeof raw === "string" ? JSON.parse(raw) : undefined;
}

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

describe("fetchSetupStatus", () => {
  test("GETs /auth/setup/status and parses the body", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({ setup_required: true, local_auth_enabled: true, oidc_enabled: false }),
          { status: 200, headers: { "Content-Type": "application/json" } },
        ),
      );

    const status = await fetchSetupStatus();

    expect(status).toEqual({
      setup_required: true,
      local_auth_enabled: true,
      oidc_enabled: false,
    });
    const [input, init] = fetchSpy.mock.calls[0] ?? [];
    expect(input).toBe("/auth/setup/status");
    expect((init?.method ?? "GET").toUpperCase()).toBe("GET");
  });

  test("rejects a body that does not match the schema", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(JSON.stringify({ setup_required: "yes" }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(fetchSetupStatus()).rejects.toThrow();
  });
});

describe("loginLocal", () => {
  test("POSTs credentials then refreshes the CSRF token", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      // 1: the login itself (204 no body)
      .mockResolvedValueOnce(new Response(null, { status: 204 }))
      // 2: refreshCsrfToken reads /auth/me
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ csrf_token: "tok123" }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }),
      );

    await loginLocal("user@example.com", "secret");

    const [input, init] = fetchSpy.mock.calls[0] ?? [];
    expect(input).toBe("/auth/local/login");
    expect(init?.method).toBe("POST");
    expect(bodyOf(init)).toEqual({ email: "user@example.com", password: "secret" });
    // The follow-up CSRF hydration ran.
    expect(fetchSpy.mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  test("throws ApiError on bad credentials and does not refresh", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response("nope", { status: 422 }));

    await expect(loginLocal("user@example.com", "wrong")).rejects.toBeInstanceOf(ApiError);
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });
});

describe("setupAdmin", () => {
  test("POSTs /auth/setup mapping displayName to display_name", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response(null, { status: 201 }));

    await setupAdmin("admin@example.com", "Admin", "a strong password");

    const [input, init] = fetchSpy.mock.calls[0] ?? [];
    expect(input).toBe("/auth/setup");
    expect(init?.method).toBe("POST");
    expect(bodyOf(init)).toEqual({
      email: "admin@example.com",
      display_name: "Admin",
      password: "a strong password",
    });
  });
});

describe("requestPasswordReset", () => {
  test("POSTs /auth/forgot-password with the email", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response(null, { status: 200 }));

    await requestPasswordReset("user@example.com");

    const [input, init] = fetchSpy.mock.calls[0] ?? [];
    expect(input).toBe("/auth/forgot-password");
    expect(init?.method).toBe("POST");
    expect(bodyOf(init)).toEqual({ email: "user@example.com" });
  });
});

describe("resetPassword", () => {
  test("POSTs /auth/reset-password mapping newPassword to new_password", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response(null, { status: 200 }));

    await resetPassword("user@example.com", "0123456789", "a brand new password");

    const [input, init] = fetchSpy.mock.calls[0] ?? [];
    expect(input).toBe("/auth/reset-password");
    expect(init?.method).toBe("POST");
    expect(bodyOf(init)).toEqual({
      email: "user@example.com",
      pin: "0123456789",
      new_password: "a brand new password",
    });
  });
});

describe("request payload validation", () => {
  test("loginLocal rejects an invalid email without calling fetch", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    await expect(loginLocal("not-an-email", "secret")).rejects.toThrow();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  test("setupAdmin rejects a blank display name without calling fetch", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    await expect(setupAdmin("admin@example.com", "   ", "a strong password")).rejects.toThrow();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  test("requestPasswordReset rejects an empty email without calling fetch", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    await expect(requestPasswordReset("")).rejects.toThrow();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  test("resetPassword rejects a too-short new password without calling fetch", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    await expect(resetPassword("user@example.com", "0123456789", "short")).rejects.toThrow();
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
