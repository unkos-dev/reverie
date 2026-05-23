/**
 * Behavioural tests for the root error boundary.
 *
 * Each test mounts a one-route memory router whose loader or component
 * fails in a specific way, then asserts that the boundary surfaces the
 * branded fallback rather than letting react-router fall through to
 * its unstyled default.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import {
  RouterProvider,
  createMemoryRouter,
  type LoaderFunction,
  type RouteObject,
} from "react-router";
import type { ReactElement } from "react";

import { ApiError } from "@/api";

import { RootErrorBoundary } from "./RootErrorBoundary";

function renderRouterWithFailure(loader: LoaderFunction): void {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const routes: RouteObject[] = [
    {
      path: "/",
      element: <p>HOME</p>,
      errorElement: <RootErrorBoundary />,
      children: [{ path: "fail", loader, element: <p>NEVER</p> }],
    },
  ];
  const router = createMemoryRouter(routes, { initialEntries: ["/fail"] });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  render(<Wrapper />);
}

describe("RootErrorBoundary", () => {
  test("renders a branded 'Not found' surface when the loader throws a 404 Response", async () => {
    renderRouterWithFailure(() => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- react-router loaders bail out via `throw new Response(...)`.
      throw new Response("nope", { status: 404 });
    });

    expect(await screen.findByRole("heading", { name: /not found/i })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /back to library/i })).toBeInTheDocument();
  });

  test("renders a numbered 'Error N' surface for other route response statuses", async () => {
    renderRouterWithFailure(() => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- react-router loaders bail out via `throw new Response(...)`.
      throw new Response("server fault", { status: 500, statusText: "Server Fault" });
    });

    expect(await screen.findByRole("heading", { name: /error 500/i })).toBeInTheDocument();
    expect(screen.getByText(/server fault/i)).toBeInTheDocument();
  });

  test("renders the ApiError title + detail when a loader throws an ApiError", async () => {
    renderRouterWithFailure(() => {
      throw new ApiError(
        503,
        "https://reverie.example/probs/internal",
        "Service Unavailable",
        "Try again in a moment.",
      );
    });

    expect(
      await screen.findByRole("heading", { name: /service unavailable/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/try again in a moment/i)).toBeInTheDocument();
  });

  test("renders the generic surface and logs the raw error for unknown exceptions", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    renderRouterWithFailure(() => {
      throw new TypeError("network down");
    });

    expect(
      await screen.findByRole("heading", { name: /something went wrong/i }),
    ).toBeInTheDocument();
    expect(errorSpy).toHaveBeenCalled();
    errorSpy.mockRestore();
  });
});
