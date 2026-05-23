import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { ApiError } from "@/api";

import App from "./App";
import { queryClient, setUnauthenticatedHandler } from "./lib/query/client";

const originalLocation = window.location;

beforeEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
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
  test("redirects to /auth/login (full page nav) on ApiError 401", async () => {
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
      expect(loc.assign).toHaveBeenCalledWith("/auth/login");
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
});
