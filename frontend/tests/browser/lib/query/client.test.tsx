import { QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";
import { page } from "vite-plus/test/browser";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { render } from "vitest-browser-react";

import { ApiError } from "@/api";
import { useAuthMe } from "@/hooks/useAuthMe";
import { useSessionRecovery } from "@/hooks/useSessionRecovery";
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

function RecoveryProbe(): ReactElement {
  useSessionRecovery();
  const { isError } = useAuthMe();
  return <output>{isError ? "identity-error" : "identity-pending-or-settled"}</output>;
}

async function renderRecoveryProbe(): Promise<void> {
  await render(
    <QueryClientProvider client={queryClient}>
      <RecoveryProbe />
    </QueryClientProvider>,
  );
}

describe("useSessionRecovery — identity check", () => {
  test.each([401, 403])(
    "an identity check answering %i navigates to sign-in once",
    async (status) => {
      const handler = vi.fn();
      setUnauthenticatedHandler(handler);
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValueOnce(new Response(null, { status }));

      await renderRecoveryProbe();

      await expect.poll(() => handler.mock.calls.length).toBe(1);
      expect(fetchSpy).toHaveBeenCalledTimes(1);
      expect(fetchSpy.mock.calls[0]?.[0]).toBe("/auth/me");
    },
  );

  test.each([
    ["a 500", () => Promise.resolve(new Response(null, { status: 500 }))],
    ["a network error", () => Promise.reject(new TypeError("Failed to fetch"))],
  ])("%s from the identity check does not navigate to sign-in", async (_, respond) => {
    const handler = vi.fn();
    setUnauthenticatedHandler(handler);
    vi.spyOn(globalThis, "fetch").mockImplementationOnce(respond);

    await renderRecoveryProbe();

    await expect.element(page.getByText("identity-error")).toBeVisible();
    expect(handler).not.toHaveBeenCalled();
  });
});
