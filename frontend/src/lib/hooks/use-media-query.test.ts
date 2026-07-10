import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";

import { useMediaQuery } from "./use-media-query";

interface MatchMediaShim {
  trigger: (matches: boolean) => void;
  listenerCount: () => number;
}

function installMatchMedia(initial: boolean): MatchMediaShim {
  let matches = initial;
  type Listener = (event: MediaQueryListEvent) => void;
  const listeners = new Set<Listener>();
  const mql = {
    get matches(): boolean {
      return matches;
    },
    media: "(min-width: 1024px)",
    addEventListener: vi.fn((_event: string, listener: Listener) => {
      listeners.add(listener);
    }),
    removeEventListener: vi.fn((_event: string, listener: Listener) => {
      listeners.delete(listener);
    }),
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => true,
    onchange: null,
  } as unknown as MediaQueryList;
  vi.stubGlobal("matchMedia", () => mql);
  return {
    trigger: (next: boolean) => {
      matches = next;
      const ev = { matches } as MediaQueryListEvent;
      for (const l of listeners) l(ev);
    },
    listenerCount: () => listeners.size,
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("useMediaQuery", () => {
  test("reflects the initial match state", () => {
    installMatchMedia(true);
    const { result } = renderHook(() => useMediaQuery("(min-width: 1024px)"));
    expect(result.current).toBe(true);
  });

  test("updates when the media query change event fires", () => {
    const shim = installMatchMedia(false);
    const { result } = renderHook(() => useMediaQuery("(min-width: 1024px)"));
    expect(result.current).toBe(false);
    act(() => {
      shim.trigger(true);
    });
    expect(result.current).toBe(true);
  });

  test("unsubscribes on unmount", () => {
    const shim = installMatchMedia(false);
    const { unmount } = renderHook(() => useMediaQuery("(min-width: 1024px)"));
    expect(shim.listenerCount()).toBe(1);
    unmount();
    expect(shim.listenerCount()).toBe(0);
  });

  test("returns false when matchMedia is unavailable", () => {
    vi.stubGlobal("matchMedia", undefined);
    const { result } = renderHook(() => useMediaQuery("(min-width: 1024px)"));
    expect(result.current).toBe(false);
  });
});
