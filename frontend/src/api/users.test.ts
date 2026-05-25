import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { __resetCsrfTokenForTesting } from "./csrf";
import { listUsers, updateUserRole, updateUserChildStatus, updateUser } from "./users";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

const STUB_USER = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  display_name: "Alice",
  email: "alice@example.com",
  role: "admin",
  is_child: false,
  created_at: "2026-05-25T00:00:00Z",
  updated_at: "2026-05-25T00:00:00Z",
};

beforeEach(() => {
  __resetCsrfTokenForTesting();
  vi.restoreAllMocks();
});

afterEach(() => {
  __resetCsrfTokenForTesting();
});

describe("listUsers", () => {
  test("returns parsed user array", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse([STUB_USER]));
    const result = await listUsers();
    expect(result).toHaveLength(1);
    expect(result[0].display_name).toBe("Alice");
    expect(result[0].role).toBe("admin");
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
    expect(call[0]).toBe(`/api/users/${STUB_USER.id}/role`);
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
    expect(call[0]).toBe(`/api/users/${STUB_USER.id}`);
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
