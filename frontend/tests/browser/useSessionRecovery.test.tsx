import { renderHook } from "vitest-browser-react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { STUB_ME } from "@/__fixtures__/auth";

import { useAuthMe } from "@/hooks/useAuthMe";
import { useSessionRecovery } from "@/hooks/useSessionRecovery";

const mockInvoke = vi.fn();

vi.mock("@/lib/query/client", () => ({
  invokeUnauthenticatedHandler: () => {
    mockInvoke();
  },
}));

function makeWrapper() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return ({ children }: { children: React.ReactNode }) =>
    createElement(QueryClientProvider, { client }, children);
}

async function renderRecovery() {
  return await renderHook(
    () => {
      useSessionRecovery();
      return useAuthMe();
    },
    { wrapper: makeWrapper() },
  );
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

    await renderRecovery();

    await vi.waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledTimes(1);
    });
  });

  test("fires recovery when /auth/me settles 403", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(new Response(null, { status: 403 }));

    await renderRecovery();

    await vi.waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledTimes(1);
    });
  });

  test("does NOT fire when /auth/me settles 200", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(JSON.stringify(STUB_ME), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    const { result } = await renderRecovery();

    await vi.waitFor(() => {
      expect(result.current.data).toBeDefined();
    });
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  test("does NOT fire when /auth/me settles 500 (operational error)", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(null, { status: 500, statusText: "Internal Server Error" }),
    );

    const { result } = await renderRecovery();

    await vi.waitFor(() => {
      expect(result.current.isError).toBe(true);
    });
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  test("does NOT fire while query is loading", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() => new Promise(() => {}));

    await renderRecovery();

    expect(mockInvoke).not.toHaveBeenCalled();
  });
});
