import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement, ReactNode } from "react";

import { ApiError } from "@/api";
import { STUB_ME } from "@/__fixtures__/auth";

import App from "./App";
import { queryClient, setUnauthenticatedHandler } from "./lib/query/client";
import { queryKeys } from "./lib/query/keys";

// The shell's own behavior (rail, drawer, admin zone) is covered in
// components/shell/*.test.tsx — here it would only drag auth/shelves
// fetches into a test about the 401 boundary.
vi.mock("@/components/shell/AppShell", () => ({
  AppShell: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

const originalLocation = window.location;

// `App` now consumes the shared `/auth/me` query via useSessionRecovery, so
// every test must answer it. Default to an authenticated 200 so recovery stays
// quiet and the QueryCache.onError funnel tests below stay isolated; the
// cold-load test overrides this with a 401.
function mockAuthMe(status: number): void {
  vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (url.includes("/auth/me")) {
      const body = status === 200 ? JSON.stringify(STUB_ME) : null;
      return Promise.resolve(
        new Response(body, { status, headers: { "Content-Type": "application/json" } }),
      );
    }
    return Promise.reject(new Error(`unexpected fetch: ${url}`));
  });
}

beforeEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
  // Seed provider state so the provider-aware redirect resolves without a
  // fetch (the redirect reads `/auth/setup/status` via `ensureQueryData`).
  // Default is OIDC-off, so the redirect target is the local form `/login`.
  queryClient.setQueryData(queryKeys.auth.setupStatus(), {
    setup_required: false,
    local_auth_enabled: true,
    oidc_enabled: false,
  });
  mockAuthMe(200);
});

afterEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
  Object.defineProperty(window, "location", {
    configurable: true,
    writable: true,
    value: originalLocation,
  });
  vi.restoreAllMocks();
});

interface MockLocation {
  assign: ReturnType<typeof vi.fn>;
  href: string;
}

function mockLocation(): MockLocation {
  const loc: MockLocation = { assign: vi.fn(), href: "http://localhost/" };
  Object.defineProperty(window, "location", {
    configurable: true,
    writable: true,
    value: loc,
  });
  return loc;
}

function renderApp(): void {
  const routes: RouteObject[] = [
    {
      path: "/",
      element: <App />,
      children: [{ index: true, element: <p>HOME_ROUTE_RENDERED</p> }],
    },
  ];
  const router = createMemoryRouter(routes, { initialEntries: ["/"] });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  render(<Wrapper />);
}

describe("App — auth boundary", () => {
  test("redirects to /login (full page nav) on ApiError 401", async () => {
    const loc = mockLocation();
    renderApp();
    expect(await screen.findByText("HOME_ROUTE_RENDERED")).toBeInTheDocument();

    await queryClient
      .fetchQuery({
        queryKey: ["__app-test", "401"],
        queryFn: () => {
          throw new ApiError(401, null, "Unauthorized", "");
        },
        retry: false,
      })
      .catch(() => {});

    await waitFor(() => {
      expect(loc.assign).toHaveBeenCalledWith("/login");
    });
  });

  test("does NOT redirect on a non-401 error", async () => {
    const loc = mockLocation();
    renderApp();
    expect(await screen.findByText("HOME_ROUTE_RENDERED")).toBeInTheDocument();

    await queryClient
      .fetchQuery({
        queryKey: ["__app-test", "500"],
        queryFn: () => {
          throw new ApiError(500, null, "Internal Server Error", "");
        },
        retry: false,
      })
      .catch(() => {});

    await new Promise((r) => setTimeout(r, 0));
    expect(loc.assign).not.toHaveBeenCalled();
  });

  test("cold-load lapsed session (GET /auth/me 401) redirects to /login", async () => {
    mockAuthMe(401);
    const loc = mockLocation();
    renderApp();

    await waitFor(() => {
      expect(loc.assign).toHaveBeenCalledWith("/login");
    });
  });

  test("OIDC-on lapse redirects to /auth/oidc/login (silent re-auth preserved)", async () => {
    queryClient.setQueryData(queryKeys.auth.setupStatus(), {
      setup_required: false,
      local_auth_enabled: true,
      oidc_enabled: true,
    });
    mockAuthMe(401);
    const loc = mockLocation();
    renderApp();

    await waitFor(() => {
      expect(loc.assign).toHaveBeenCalledWith("/auth/oidc/login");
    });
  });

  test("navigates once when the me-query 401s and an ApiError 401 also fires", async () => {
    mockAuthMe(401);
    const loc = mockLocation();
    renderApp();

    await waitFor(() => {
      expect(loc.assign).toHaveBeenCalledWith("/login");
    });

    await queryClient
      .fetchQuery({
        queryKey: ["__app-test", "concurrent-401"],
        queryFn: () => {
          throw new ApiError(401, null, "Unauthorized", "");
        },
        retry: false,
      })
      .catch(() => {});

    await new Promise((r) => setTimeout(r, 0));
    expect(loc.assign).toHaveBeenCalledTimes(1);
  });
});
