/**
 * Behavioural tests for the root error boundary.
 *
 * Each test mounts a one-route memory router whose loader or component
 * fails in a specific way, then asserts that the boundary surfaces the
 * branded fallback rather than letting react-router fall through to
 * its unstyled default.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { describe, expect, test, vi } from "vite-plus/test";
import {
  RouterProvider,
  createMemoryRouter,
  type LoaderFunction,
  type RouteObject,
} from "react-router";
import type { ReactElement } from "react";

import { ApiError } from "@/api";

import { RootErrorBoundary } from "@/components/RootErrorBoundary";

async function renderRouterWithFailure(
  loader: LoaderFunction,
): Promise<ReturnType<typeof createMemoryRouter>> {
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
  await render(<Wrapper />);
  return router;
}

describe("RootErrorBoundary", () => {
  test("renders a branded 'Not found' surface when the loader throws a 404 Response", async () => {
    const router = await renderRouterWithFailure(() => {
      // oxlint-disable-next-line typescript/only-throw-error -- react-router loaders bail out via `throw new Response(...)`.
      throw new Response("nope", { status: 404 });
    });

    await expect.element(page.getByRole("heading", { name: /not found/i })).toBeInTheDocument();
    const link = page.getByRole("link", { name: /back to library/i });
    await expect.element(link).toHaveAttribute("href", "/library");
    await expect
      .element(page.getByRole("button", { name: /back to library/i }))
      .not.toBeInTheDocument();
    const user = userEvent;
    await user.keyboard("{Control>}");
    await user.click(link);
    await user.keyboard("{/Control}");
    expect(router.state.location.pathname).toBe("/fail");
    await user.click(link);
    await expect.element(page.getByText("Library destination")).toBeInTheDocument();
  });

  test("renders a numbered 'Error N' surface for other route response statuses", async () => {
    await renderRouterWithFailure(() => {
      // oxlint-disable-next-line typescript/only-throw-error -- react-router loaders bail out via `throw new Response(...)`.
      throw new Response("server fault", { status: 500, statusText: "Server Fault" });
    });

    await expect.element(page.getByRole("heading", { name: /error 500/i })).toBeInTheDocument();
    await expect.element(page.getByText(/server fault/i)).toBeInTheDocument();
  });

  test("renders the ApiError title + detail when a loader throws an ApiError", async () => {
    await renderRouterWithFailure(() => {
      throw new ApiError(
        503,
        "https://reverie.example/probs/internal",
        "Service Unavailable",
        "Try again in a moment.",
      );
    });

    await expect
      .element(page.getByRole("heading", { name: /service unavailable/i }))
      .toBeInTheDocument();
    await expect.element(page.getByText(/try again in a moment/i)).toBeInTheDocument();
  });

  test("renders the generic surface and logs the raw error for unknown exceptions", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    await renderRouterWithFailure(() => {
      throw new TypeError("network down");
    });

    await expect
      .element(page.getByRole("heading", { name: /something went wrong/i }))
      .toBeInTheDocument();
    expect(errorSpy).toHaveBeenCalled();
    errorSpy.mockRestore();
  });
});
