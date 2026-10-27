/**
 * Lazy-load + Suspense-fallback contract for the dev-only devtools
 * panel. The panel is gated behind `import.meta.env.DEV` in `main.tsx`
 * (dead code in prod builds); this test verifies the lazy + Suspense
 * shape doesn't regress, so a dev hot-reload that fails to resolve
 * `@tanstack/react-query-devtools` doesn't crash the route tree.
 */
import { render } from "vitest-browser-react";
import { page } from "vite-plus/test/browser";
import { describe, expect, test, vi } from "vite-plus/test";

vi.mock("@tanstack/react-query-devtools", () => ({
  ReactQueryDevtools: () => <div data-testid="devtools-mock">devtools</div>,
}));

describe("ReactQueryDevtoolsPanel", () => {
  test("renders the lazily-imported devtools inside a Suspense boundary", async () => {
    const { ReactQueryDevtoolsPanel } = await import("@/lib/query/devtools");
    await render(<ReactQueryDevtoolsPanel />);
    await expect.element(page.getByTestId("devtools-mock")).toBeInTheDocument();
  });
});
