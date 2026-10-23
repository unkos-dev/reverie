import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import {
  fetchSetupStatus,
  loginLocal,
  lookupCredentialLink,
  logout,
  requestPasswordReset,
  setupAdmin,
} from "./auth";
import { __seedCsrfTokenForTesting } from "./csrf";
import { ApiError } from "./errors";

describe("credential-link account email", () => {
  test.each([
    "john@example.com",
    "john.doe+books@example.com",
    "o'brien@example.com",
    "Jane_Doe@EXAMPLE.COM",
  ])("accepts %s", async (email) => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(JSON.stringify({ email, has_password: false }), { status: 200 }),
    );
    await expect(lookupCredentialLink("test-link")).resolves.toEqual({
      email,
      has_password: false,
    });
  });

  test.each([
    '"john doe"@example.com',
    '"john"@example.com',
    "jane@homelab",
    "jane!books@example.com",
    "jane#books@example.com",
    "jane%books@example.com",
    "jane/books@example.com",
    "josé@example.com",
  ])("rejects %s", async (email) => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(JSON.stringify({ email, has_password: false }), { status: 200 }),
    );
    await expect(lookupCredentialLink("test-link")).rejects.toThrow();
  });
});

/** Parse the JSON body passed to a mocked `fetch` call. */
function bodyOf(init: RequestInit | undefined): unknown {
  const raw = init?.body;
  return typeof raw === "string" ? JSON.parse(raw) : undefined;
}

beforeEach(() => {
  // Seed (not reset): apiFetch lazily hydrates an empty cache with a
  // leading /auth/me fetch that would eat these suites' response mocks;
  // hydration behaviour itself is pinned in fetch.test.ts.
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
  vi.restoreAllMocks();
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
  test("validates policy shape without duplicating server configuration floors", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          setup_required: false,
          local_auth_enabled: true,
          oidc_enabled: false,
          email_recovery_enabled: false,
          password_min_length: 8,
          password_max_length: 32,
        }),
        { status: 200 },
      ),
    );
    await expect(fetchSetupStatus()).resolves.toMatchObject({
      password_min_length: 8,
      password_max_length: 32,
    });
  });

  test.each([
    {},
    { password_min_length: 0, password_max_length: 256 },
    { password_min_length: 15.5, password_max_length: 256 },
    { password_min_length: 24, password_max_length: "256" },
  ])("refuses missing or malformed password policy %j", async (policy) => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          setup_required: true,
          local_auth_enabled: true,
          oidc_enabled: false,
          email_recovery_enabled: false,
          ...policy,
        }),
        { status: 200 },
      ),
    );
    await expect(fetchSetupStatus()).rejects.toThrow();
  });

  test("GETs /auth/setup/status and parses the body", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          setup_required: true,
          local_auth_enabled: true,
          password_min_length: 15,
          password_max_length: 256,
          oidc_enabled: false,
          email_recovery_enabled: false,
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );

    const status = await fetchSetupStatus();

    expect(status).toEqual({
      setup_required: true,
      local_auth_enabled: true,
      password_min_length: 15,
      password_max_length: 256,
      oidc_enabled: false,
      email_recovery_enabled: false,
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
});

describe("mailbox verification", () => {
  test("parses only the current address and instance mail capability", async () => {
    const { fetchEmailVerification } = await import("./auth");
    const status = { email: "holder@example.test", email_verified: false, mail_enabled: true };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(JSON.stringify(status), { status: 200 }),
    );
    expect(await fetchEmailVerification()).toEqual(status);
    expect(vi.mocked(fetch).mock.calls[0][0]).toBe("/api/v1/account/email-verifications");
  });
  test("requests and confirms verification using session-protected POST bodies", async () => {
    const { requestEmailVerification, confirmEmailVerification } = await import("./auth");
    const request = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(new Response(null, { status: 204 }));
    await requestEmailVerification();
    await confirmEmailVerification("A".repeat(43));
    expect(request.mock.calls[0][0]).toBe("/api/v1/account/email-verifications");
    expect(request.mock.calls[0][1]?.method).toBe("POST");
    expect(request.mock.calls[1][0]).toBe("/api/v1/account/email-verifications/confirmations");
    expect(bodyOf(request.mock.calls[1][1])).toEqual({ token: "A".repeat(43) });
  });
  test("refuses malformed tokens and malformed response addresses", async () => {
    const { fetchEmailVerification, confirmEmailVerification } = await import("./auth");
    const request = vi.spyOn(globalThis, "fetch");
    await expect(confirmEmailVerification("not-a-token")).rejects.toThrow();
    expect(request).not.toHaveBeenCalled();
    request.mockResolvedValueOnce(
      new Response(
        JSON.stringify({ email: "invalid", email_verified: false, mail_enabled: true }),
        { status: 200 },
      ),
    );
    await expect(fetchEmailVerification()).rejects.toThrow();
  });
});
