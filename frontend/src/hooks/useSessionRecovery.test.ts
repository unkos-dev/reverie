import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { STUB_ME } from "@/__fixtures__/auth";

import { useAuthMe } from "./useAuthMe";
import { useSessionRecovery } from "./useSessionRecovery";

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

// Drive the hook under test and expose the shared `/auth/me` state (same cached
// query) so negative tests can wait for a SETTLED state before asserting the
// recovery never fired, rather than racing a bare event-loop tick.
function renderRecovery() {
  return renderHook(
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

    renderRecovery();

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledTimes(1);
    });
  });

  test("fires recovery when /auth/me settles 403", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(new Response(null, { status: 403 }));

    renderRecovery();

    await waitFor(() => {
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

    const { result } = renderRecovery();

    await waitFor(() => {
      expect(result.current.data).toBeDefined();
    });
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  test("does NOT fire when /auth/me settles 500 (operational error)", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(null, { status: 500, statusText: "Internal Server Error" }),
    );

    const { result } = renderRecovery();

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  test("does NOT fire while query is loading", () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() => new Promise(() => {}));

    renderRecovery();

    expect(mockInvoke).not.toHaveBeenCalled();
  });
});
