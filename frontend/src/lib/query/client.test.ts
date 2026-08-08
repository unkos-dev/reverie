import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { ApiError } from "@/api";
import { activeUserId, rememberActiveUser } from "@/lib/active-user";

import { invokeUnauthenticatedHandler, queryClient, setUnauthenticatedHandler } from "./client";

beforeEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
});

afterEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
  vi.restoreAllMocks();
});

describe("queryClient — QueryCache onError", () => {
  test("calls the unauthenticated handler on ApiError 401", async () => {
    const handler = vi.fn();
    setUnauthenticatedHandler(handler);

    await queryClient
      .fetchQuery({
        queryKey: ["__test", "401"],
        queryFn: () => {
          throw new ApiError(401, "https://reverie.example/probs/unauthorized", "Unauthorized", "");
        },
        retry: false,
      })
      .catch(() => {
        /* swallow — assertion is on the handler call below */
      });

    expect(handler).toHaveBeenCalledTimes(1);
  });

  test("does NOT call the handler on ApiError 500", async () => {
    const handler = vi.fn();
    setUnauthenticatedHandler(handler);

    await queryClient
      .fetchQuery({
        queryKey: ["__test", "500"],
        queryFn: () => {
          throw new ApiError(500, null, "Internal Server Error", "");
        },
        retry: false,
      })
      .catch(() => {});

    expect(handler).not.toHaveBeenCalled();
  });

  test("does NOT call the handler on a non-ApiError exception", async () => {
    const handler = vi.fn();
    setUnauthenticatedHandler(handler);

    await queryClient
      .fetchQuery({
        queryKey: ["__test", "TypeError"],
        queryFn: () => {
          throw new TypeError("network down");
        },
        retry: false,
      })
      .catch(() => {});

    expect(handler).not.toHaveBeenCalled();
  });

  test("setUnauthenticatedHandler replaces the previous handler", async () => {
    const first = vi.fn();
    const second = vi.fn();
    setUnauthenticatedHandler(first);
    setUnauthenticatedHandler(second);

    await queryClient
      .fetchQuery({
        queryKey: ["__test", "replace"],
        queryFn: () => {
          throw new ApiError(401, null, "Unauthorized", "");
        },
        retry: false,
      })
      .catch(() => {});

    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });
});

describe("invokeUnauthenticatedHandler — once-guard", () => {
  test("fires the wired handler once", () => {
    const handler = vi.fn();
    setUnauthenticatedHandler(handler);

    invokeUnauthenticatedHandler();

    expect(handler).toHaveBeenCalledTimes(1);
  });

  test("is guarded — repeated calls navigate once", () => {
    const handler = vi.fn();
    setUnauthenticatedHandler(handler);

    invokeUnauthenticatedHandler();
    invokeUnauthenticatedHandler();
    invokeUnauthenticatedHandler();

    expect(handler).toHaveBeenCalledTimes(1);
  });

  test("setUnauthenticatedHandler resets the once-guard", () => {
    const first = vi.fn();
    const second = vi.fn();

    setUnauthenticatedHandler(first);
    invokeUnauthenticatedHandler();
    expect(first).toHaveBeenCalledTimes(1);

    setUnauthenticatedHandler(second);
    invokeUnauthenticatedHandler();
    expect(second).toHaveBeenCalledTimes(1);
  });

  test("a dead session forgets the browser's active user", () => {
    // The expiry path never runs the sign-out flow, so this is the only
    // hook that stops one account's per-user caches from resolving while
    // a different account signs in next.
    rememberActiveUser("user-a");
    setUnauthenticatedHandler(() => {});

    invokeUnauthenticatedHandler();

    expect(activeUserId()).toBeNull();
    localStorage.clear();
  });
});
