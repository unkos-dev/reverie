/**
 * Behavioural tests for the root error boundary.
 *
 * Each test mounts a one-route memory router whose loader or component
 * fails in a specific way, then asserts that the boundary surfaces the
 * branded fallback rather than letting react-router fall through to
 * its unstyled default.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import userEvent from "@testing-library/user-event";
import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vite-plus/test";
import {
  RouterProvider,
  createMemoryRouter,
  type LoaderFunction,
  type RouteObject,
} from "react-router";
import type { ReactElement } from "react";

import { ApiError } from "@/api";

import { RootErrorBoundary } from "./RootErrorBoundary";

function renderRouterWithFailure(loader: LoaderFunction): ReturnType<typeof createMemoryRouter> {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const routes: RouteObject[] = [
    {
      path: "/",
      element: <p>HOME</p>,
      errorElement: <RootErrorBoundary />,
      children: [{ path: "fail", loader, element: <p>NEVER</p> }],
    },
    { path: "/library", element: <p>Library destination</p> },
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
  return router;
}

describe("RootErrorBoundary", () => {
  test("renders a branded 'Not found' surface when the loader throws a 404 Response", async () => {
    const router = renderRouterWithFailure(() => {
      // oxlint-disable-next-line typescript/only-throw-error -- react-router loaders bail out via `throw new Response(...)`.
      throw new Response("nope", { status: 404 });
    });

    expect(await screen.findByRole("heading", { name: /not found/i })).toBeInTheDocument();
    const link = screen.getByRole("link", { name: /back to library/i });
    expect(link).toHaveAttribute("href", "/library");
    expect(screen.queryByRole("button", { name: /back to library/i })).not.toBeInTheDocument();
    const user = userEvent.setup();
    await user.keyboard("{Control>}");
    await user.click(link);
    await user.keyboard("{/Control}");
    expect(router.state.location.pathname).toBe("/fail");
    await user.click(link);
    expect(await screen.findByText("Library destination")).toBeInTheDocument();
  });

  test("renders a numbered 'Error N' surface for other route response statuses", async () => {
    renderRouterWithFailure(() => {
      // oxlint-disable-next-line typescript/only-throw-error -- react-router loaders bail out via `throw new Response(...)`.
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
