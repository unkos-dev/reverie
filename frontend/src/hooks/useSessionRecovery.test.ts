import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { useSessionRecovery } from "./useSessionRecovery";

const mockInvoke = vi.fn();

vi.mock("@/lib/query/client", () => ({
  invokeUnauthenticatedHandler: () => {
    mockInvoke();
  },
}));

const STUB_ME = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  display_name: "Alice",
  email: "alice@example.com",
  role: "admin" as const,
  is_child: false,
  theme_preference: "system",
  csrf_token: null,
};

function makeWrapper() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return ({ children }: { children: React.ReactNode }) =>
    createElement(QueryClientProvider, { client }, children);
}

beforeEach(() => {
  mockInvoke.mockClear();
  vi.restoreAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("useSessionRecovery", () => {
  test("fires recovery when /auth/me settles 401", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(new Response(null, { status: 401 }));

    renderHook(
      () => {
        useSessionRecovery();
      },
      { wrapper: makeWrapper() },
    );

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledTimes(1);
    });
  });

  test("fires recovery when /auth/me settles 403", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(new Response(null, { status: 403 }));

    renderHook(
      () => {
        useSessionRecovery();
      },
      { wrapper: makeWrapper() },
    );

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledTimes(1);
    });
  });

  test("does NOT fire when /auth/me returns 200", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(JSON.stringify(STUB_ME), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    renderHook(
      () => {
        useSessionRecovery();
      },
      { wrapper: makeWrapper() },
    );

    await new Promise((r) => setTimeout(r, 0));
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  test("does NOT fire on 500 (operational error)", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(null, { status: 500, statusText: "Internal Server Error" }),
    );

    renderHook(
      () => {
        useSessionRecovery();
      },
      { wrapper: makeWrapper() },
    );

    await new Promise((r) => setTimeout(r, 0));
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  test("does NOT fire while query is loading", () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() => new Promise(() => {}));

    renderHook(
      () => {
        useSessionRecovery();
      },
      { wrapper: makeWrapper() },
    );

    expect(mockInvoke).not.toHaveBeenCalled();
  });
});
