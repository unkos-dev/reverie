import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NuqsAdapter } from "nuqs/adapters/react-router/v8";
import type { ReactElement } from "react";
import { createBrowserRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, test } from "vite-plus/test";

import { FILTER_PARAM_KEYS, PURGE_ONLY_PARAM_KEYS } from "@/routes/library-params";

import { ALL_FILTER_KEYS, useLibraryFilters } from "./use-library-filters";

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
      <button
        type="button"
        onClick={() => {
          // Two values in one set family, to prove the key survives as a
          // repeated param rather than collapsing to the last one.
          commitSlice("genres", (current) => ({
            ...current,
            genres: { ...current.genres, any: ["scifi", "fantasy"] },
          }));
          commitSlice("status", (current) => ({
            ...current,
            status: { ...current.status, any: ["reading", "finished"] },
          }));
        }}
      >
        pick-multi
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

  test("the clear-all census matches the codec's, in both directions", () => {
    // The transport keeps its own list of every key it may write, because
    // the codec's is `readonly string[]` and cannot be narrowed to this
    // module's key union without a cast or a silent drop. Two lists means
    // they can drift, so this holds them together: a filter the codec
    // parses but clear-all never sweeps stays set forever, and a key here
    // that the codec no longer knows is dead weight nothing will catch.
    //
    // Every entry having a parser is already proven by the type, so it is
    // deliberately not re-asserted here.
    const canonical = new Set([...FILTER_PARAM_KEYS, ...PURGE_ONLY_PARAM_KEYS]);
    const census = new Set<string>(ALL_FILTER_KEYS);

    expect([...canonical].filter((key) => !census.has(key))).toEqual([]);
    expect([...census].filter((key) => !canonical.has(key))).toEqual([]);
  });

  test("every set-valued key round-trips as a repeated param", async () => {
    renderProbe();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "pick-multi" }));
    await waitFor(() => {
      const search = currentSearch();
      // A key wired as a single-value parser would keep only the last
      // value here, which the codec's clamped sets would then silently
      // under-report rather than fail on.
      expect(search.getAll("genre_any")).toEqual(["scifi", "fantasy"]);
      expect(search.getAll("status_any")).toEqual(["reading", "finished"]);
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
