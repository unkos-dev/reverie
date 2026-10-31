import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { ApiError } from "@/api";
import { activeUserId, rememberActiveUser } from "@/lib/active-user";

import {
  invokeUnauthenticatedHandler,
  queryClient,
  setUnauthenticatedHandler,
} from "@/lib/query/client";

beforeEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
});

afterEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
  vi.restoreAllMocks();
});

describe("invokeUnauthenticatedHandler — once-guard", () => {
  test("is guarded — repeated calls navigate once", async () => {
    const handler = vi.fn();
    setUnauthenticatedHandler(handler);

    await Promise.all(
      ["a", "b", "c"].map((key) =>
        queryClient
          .query({
            queryKey: ["__test", "lapse", key],
            queryFn: () => {
              throw new ApiError(
                401,
                "https://reverie.example/probs/unauthorized",
                "Unauthorized",
                "",
              );
            },
            retry: false,
          })
          .catch(() => {}),
      ),
    );

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
    let hintAtRedirect: string | null | undefined;
    setUnauthenticatedHandler(() => {
      hintAtRedirect = activeUserId();
    });

    invokeUnauthenticatedHandler();

    expect(hintAtRedirect).toBeNull();
    localStorage.clear();
  });
});
