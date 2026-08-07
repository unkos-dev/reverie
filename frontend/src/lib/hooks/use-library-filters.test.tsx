import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NuqsAdapter } from "nuqs/adapters/react-router/v8";
import type { ReactElement } from "react";
import { createBrowserRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, test } from "vite-plus/test";

import { useLibraryFilters } from "./use-library-filters";

function Probe(): ReactElement {
  const { commitSlice, clearAll } = useLibraryFilters();
  return (
    <div>
      <button
        type="button"
        onClick={() => {
          // Two typed slices, one tick: both writes are debounced and both
          // are still queued when the clear below lands.
          commitSlice("title", (current) => ({ ...current, title: { contains: "dune" } }), {
            debounced: true,
          });
          commitSlice("pages", (current) => ({ ...current, pages: { gte: 300 } }), {
            debounced: true,
          });
        }}
      >
        type-two-slices
      </button>
      <button
        type="button"
        onClick={() => {
          // Different slices, one tick, neither debounced.
          commitSlice("shelf", (current) => ({ ...current, shelf: "shelf-1" }));
          commitSlice("series", (current) => ({ ...current, series: "series-1" }));
        }}
      >
        pick-two-slices
      </button>
      <button type="button" onClick={clearAll}>
        clear-all
      </button>
    </div>
  );
}

function renderProbe(initialEntry = "/library"): void {
  window.history.replaceState(null, "", initialEntry);
  const router = createBrowserRouter([
    {
      path: "/library",
      element: (
        <NuqsAdapter>
          <Probe />
        </NuqsAdapter>
      ),
    },
  ]);
  render(<RouterProvider router={router} />);
}

function currentSearch(): URLSearchParams {
  return new URLSearchParams(window.location.search);
}

async function debounceSettled(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 400));
}

afterEach(() => {
  window.history.replaceState(null, "", "/library");
});

describe("useLibraryFilters", () => {
  test("same-tick writes to different slices both land", async () => {
    renderProbe("/library?keep=yes");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "pick-two-slices" }));
    await waitFor(() => {
      const search = currentSearch();
      expect(search.get("shelf")).toBe("shelf-1");
      expect(search.get("series")).toBe("series-1");
      expect(search.get("keep")).toBe("yes");
    });
  });

  test("a filter write drops the cursor in the same update", async () => {
    renderProbe("/library?cursor=abc");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "pick-two-slices" }));
    await waitFor(() => {
      const search = currentSearch();
      expect(search.get("shelf")).toBe("shelf-1");
      // Same update, so there is no window in which a filter change is
      // observable with the stale keyset position still attached.
      expect(search.get("cursor")).toBeNull();
    });
  });

  test("clear-all cancels pending writes across every typed slice at once", async () => {
    renderProbe();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "type-two-slices" }));
    await user.click(screen.getByRole("button", { name: "clear-all" }));
    await debounceSettled();
    const search = currentSearch();
    expect(search.get("title_contains")).toBeNull();
    expect(search.get("pages_gte")).toBeNull();
  });

  test("clear-all removes conditions the URL already carried", async () => {
    renderProbe("/library?title_contains=dune&genre_any=scifi&status_any=reading&cursor=abc");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "clear-all" }));
    await waitFor(() => {
      const search = currentSearch();
      expect(search.get("title_contains")).toBeNull();
      expect(search.getAll("genre_any")).toEqual([]);
      expect(search.getAll("status_any")).toEqual([]);
      expect(search.get("cursor")).toBeNull();
    });
  });

  test("clear-all purges a dead param no wire predicate reads", async () => {
    // `title_empty` has no predicate, so nothing parses or sends it, but a
    // hand-crafted or stale URL can still carry one.
    renderProbe("/library?title_empty=true");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "clear-all" }));
    await waitFor(() => {
      expect(currentSearch().get("title_empty")).toBeNull();
    });
  });

  test("clear-all leaves sort alone; ordering is not a filter", async () => {
    renderProbe("/library?sort=title&title_contains=dune");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "clear-all" }));
    await waitFor(() => {
      const search = currentSearch();
      expect(search.get("title_contains")).toBeNull();
      expect(search.get("sort")).toBe("title");
    });
  });
});
