import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { useAuthorLabels } from "./use-author-labels";

const LE_GUIN = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function makeWrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client }, children);
}

beforeEach(() => {
  vi.restoreAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("useAuthorLabels", () => {
  test("resolves an author id to its label", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      jsonResponse({ items: [{ id: LE_GUIN, value: "Ursula K. Le Guin" }] }),
    );

    const { result } = renderHook(() => useAuthorLabels([LE_GUIN]), { wrapper: makeWrapper() });

    await waitFor(() => {
      expect(result.current.labelFor(LE_GUIN)).toBe("Ursula K. Le Guin");
    });
  });

  test("labels an id as resolving while the lookup is in flight", () => {
    vi.spyOn(globalThis, "fetch").mockReturnValue(new Promise(() => undefined));

    const { result } = renderHook(() => useAuthorLabels([LE_GUIN]), { wrapper: makeWrapper() });

    expect(result.current.labelFor(LE_GUIN)).toBe("resolving…");
  });

  test("labels an unresolved id as unknown", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(jsonResponse({ items: [] }));

    const { result } = renderHook(() => useAuthorLabels([LE_GUIN]), { wrapper: makeWrapper() });

    await waitFor(() => {
      expect(result.current.labelFor(LE_GUIN)).toBe("Unknown author");
    });
  });

  test("labels an id as unavailable when resolution fails, and logs a breadcrumb", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network down"));
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);

    const { result } = renderHook(() => useAuthorLabels([LE_GUIN]), { wrapper: makeWrapper() });

    await waitFor(() => {
      expect(result.current.labelFor(LE_GUIN)).toBe("author unavailable");
    });
    expect(errorSpy).toHaveBeenCalled();
  });

  test("does not fetch when there are no ids to resolve", () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    const { result } = renderHook(() => useAuthorLabels([]), { wrapper: makeWrapper() });

    expect(fetchSpy).not.toHaveBeenCalled();
    expect(result.current.labelFor(LE_GUIN)).toBe("Unknown author");
  });
});
