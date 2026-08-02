import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "./csrf";
import {
  listUsers,
  updateUserRole,
  updateUserChildStatus,
  updateUser,
  createUser,
  setAccountStatus,
  adminResetPassword,
} from "./users";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

/** Parse the JSON request body recorded for a mocked fetch call. */
function bodyJson(init: RequestInit | undefined): unknown {
  const body = init?.body;
  if (typeof body !== "string") throw new Error("expected a serialized JSON body");
  return JSON.parse(body);
}

const STUB_USER = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  display_name: "Alice",
  email: "alice@example.com",
  role: "admin",
  is_child: false,
  disabled: false,
  created_at: "2026-05-25T00:00:00Z",
  updated_at: "2026-05-25T00:00:00Z",
};

beforeEach(() => {
  // Seed (not reset): apiFetch lazily hydrates an empty cache with a
  // leading /auth/me fetch that would eat these suites' response mocks;
  // hydration behaviour itself is pinned in fetch.test.ts.
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
  vi.restoreAllMocks();
});

describe("listUsers", () => {
  test("returns parsed user array", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse([STUB_USER]));
    const result = await listUsers();
    expect(result).toHaveLength(1);
    expect(result[0].display_name).toBe("Alice");
    expect(result[0].role).toBe("admin");
    expect(vi.mocked(fetch).mock.calls[0][0]).toBe("/api/v1/users");
  });

  test("throws on non-2xx", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse(
        {
          type: "https://reverie.example/probs/forbidden",
          status: 403,
          title: "Forbidden",
          detail: "Access denied.",
        },
        { status: 403, headers: { "Content-Type": "application/problem+json" } },
      ),
    );
    await expect(listUsers()).rejects.toThrow();
  });
});

describe("updateUserRole", () => {
  test("sends PUT with role body", async () => {
    const updated = { ...STUB_USER, role: "adult" };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(updated));
    const result = await updateUserRole(STUB_USER.id, "adult");
    expect(result.role).toBe("adult");
    const call = vi.mocked(fetch).mock.calls[0];
    expect(call[0]).toBe(`/api/v1/users/${STUB_USER.id}/role`);
    expect(call[1]?.method).toBe("PUT");
  });

  test("throws on non-2xx", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse(
        { type: "https://reverie.example/probs/forbidden", status: 403, title: "Forbidden" },
        { status: 403, headers: { "Content-Type": "application/problem+json" } },
      ),
    );
    await expect(updateUserRole(STUB_USER.id, "adult")).rejects.toThrow();
  });

  test("throws on schema mismatch", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse({ id: "bad" }));
    await expect(updateUserRole(STUB_USER.id, "adult")).rejects.toThrow();
  });
});

describe("updateUserChildStatus", () => {
  test("sends PUT with is_child body", async () => {
    const updated = { ...STUB_USER, is_child: true, role: "child" };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(updated));
    const result = await updateUserChildStatus(STUB_USER.id, true);
    expect(result.is_child).toBe(true);
    expect(result.role).toBe("child");
    const call = vi.mocked(fetch).mock.calls[0];
    expect(call[0]).toBe(`/api/v1/users/${STUB_USER.id}/child-status`);
    expect(call[1]?.method).toBe("PUT");
  });

  test("throws on non-2xx", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse(
        { type: "https://reverie.example/probs/forbidden", status: 403, title: "Forbidden" },
        { status: 403, headers: { "Content-Type": "application/problem+json" } },
      ),
    );
    await expect(updateUserChildStatus(STUB_USER.id, true)).rejects.toThrow();
  });

  test("throws on schema mismatch", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse({ id: "bad" }));
    await expect(updateUserChildStatus(STUB_USER.id, true)).rejects.toThrow();
  });
});

describe("updateUser", () => {
  test("sends PATCH with fields body", async () => {
    const updated = { ...STUB_USER, display_name: "Bob" };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(updated));
    const result = await updateUser(STUB_USER.id, { display_name: "Bob" });
    expect(result.display_name).toBe("Bob");
    const call = vi.mocked(fetch).mock.calls[0];
    expect(call[0]).toBe(`/api/v1/users/${STUB_USER.id}`);
    expect(call[1]?.method).toBe("PATCH");
  });

  test("throws on non-2xx", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse(
        {
          type: "https://reverie.example/probs/validation",
          status: 422,
          title: "Validation Error",
          detail: "email already in use",
        },
        { status: 422, headers: { "Content-Type": "application/problem+json" } },
      ),
    );
    await expect(updateUser(STUB_USER.id, { email: "taken@example.com" })).rejects.toThrow();
  });

  test("throws on schema mismatch", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse({ id: "bad" }));
    await expect(updateUser(STUB_USER.id, { display_name: "Bob" })).rejects.toThrow();
  });
});

describe("createUser", () => {
  test("sends POST with create body and parses the created user", async () => {
    const created = {
      ...STUB_USER,
      display_name: "Carol",
      email: "carol@example.com",
      role: "adult",
    };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(created, { status: 201 }));
    const result = await createUser({
      email: "carol@example.com",
      display_name: "Carol",
      role: "adult",
      password: "correct-horse-battery-staple",
    });
    expect(result.display_name).toBe("Carol");
    expect(result.role).toBe("adult");
    const call = vi.mocked(fetch).mock.calls[0];
    expect(call[0]).toBe("/api/v1/users");
    expect(call[1]?.method).toBe("POST");
    expect(bodyJson(call[1])).toEqual({
      email: "carol@example.com",
      display_name: "Carol",
      role: "adult",
      password: "correct-horse-battery-staple",
    });
  });

  test("throws on duplicate email (409)", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse(
        { type: "https://reverie.example/probs/email-conflict", status: 409, title: "Conflict" },
        { status: 409, headers: { "Content-Type": "application/problem+json" } },
      ),
    );
    await expect(
      createUser({
        email: "taken@example.com",
        display_name: "Carol",
        role: "adult",
        password: "correct-horse-battery-staple",
      }),
    ).rejects.toThrow();
  });
});

describe("setAccountStatus", () => {
  test("sends PUT with disabled body and parses the updated user", async () => {
    const disabled = { ...STUB_USER, disabled: true };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(disabled));
    const result = await setAccountStatus(STUB_USER.id, true);
    expect(result.disabled).toBe(true);
    const call = vi.mocked(fetch).mock.calls[0];
    expect(call[0]).toBe(`/api/v1/users/${STUB_USER.id}/account-status`);
    expect(call[1]?.method).toBe("PUT");
    expect(bodyJson(call[1])).toEqual({ disabled: true });
  });

  test("throws on non-2xx", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse(
        {
          type: "https://reverie.example/probs/validation",
          status: 422,
          title: "Validation Error",
        },
        { status: 422, headers: { "Content-Type": "application/problem+json" } },
      ),
    );
    await expect(setAccountStatus(STUB_USER.id, true)).rejects.toThrow();
  });
});

describe("adminResetPassword", () => {
  test("sends POST with new_password and resolves void on empty 200", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(new Response(null, { status: 200 }));
    await expect(
      adminResetPassword(STUB_USER.id, "correct-horse-battery-staple"),
    ).resolves.toBeUndefined();
    const call = vi.mocked(fetch).mock.calls[0];
    expect(call[0]).toBe(`/api/v1/users/${STUB_USER.id}/password-reset`);
    expect(call[1]?.method).toBe("POST");
    expect(bodyJson(call[1])).toEqual({ new_password: "correct-horse-battery-staple" });
  });

  test("throws on policy rejection (422)", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse(
        {
          type: "https://reverie.example/probs/validation",
          status: 422,
          title: "Validation Error",
        },
        { status: 422, headers: { "Content-Type": "application/problem+json" } },
      ),
    );
    await expect(adminResetPassword(STUB_USER.id, "weak")).rejects.toThrow();
  });
});
