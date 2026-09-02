import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import type { AuthMe } from "@/hooks/useAuthMe";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";

import { loader } from "./admin";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

function authMe(role: AuthMe["role"]): AuthMe {
  return {
    id: "11111111-1111-4111-8111-111111111111",
    display_name: "Admin",
    email: "admin@example.com",
    role,
    is_child: false,
    theme_preference: "system",
    csrf_token: null,
  };
}

function userList(): unknown[] {
  return [
    {
      id: "22222222-2222-4222-8222-222222222222",
      display_name: "Someone",
      email: "someone@example.com",
      role: "adult",
      is_child: false,
      disabled: false,
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    },
  ];
}

beforeEach(() => {
  queryClient.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  queryClient.clear();
});

describe("admin loader", () => {
  test("seeds the user list cache when the cached identity is admin", async () => {
    queryClient.setQueryData(queryKeys.auth.me(), authMe("admin"));
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(userList()));

    await loader();

    expect(queryClient.getQueryData(queryKeys.users.list())).toBeDefined();
  });

  test("resolves without throwing when the prefetch fails, leaving the cache cold", async () => {
    queryClient.setQueryData(queryKeys.auth.me(), authMe("admin"));
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response("upstream blew up", { status: 500 }),
    );

    const result = await loader();

    expect(result).toBeNull();
    expect(queryClient.getQueryData(queryKeys.users.list())).toBeUndefined();
  });

  test("skips the prefetch entirely when the cached identity is not admin", async () => {
    queryClient.setQueryData(queryKeys.auth.me(), authMe("adult"));
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    await loader();

    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
