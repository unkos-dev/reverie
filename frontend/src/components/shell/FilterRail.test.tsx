import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  emptyFilterState,
  filterStateToParams,
  LibraryVisitSchema,
  type FilterState,
} from "@/routes/library-params";
import { useState, type ReactElement } from "react";
import { createBrowserRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import type { SortLevelParam } from "@/api";

import { FilterRail } from "./FilterRail";

const SERIES = [
  { id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", name: "Discworld" },
  { id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", name: "Culture" },
];

const SHELVES = [
  {
    id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
    name: "Wishlist",
    is_system: false,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    item_count: 3,
  },
  {
    id: "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
    name: "Favourites",
    is_system: false,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    item_count: 1,
  },
];

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function mockFetchByUrl(): void {
  vi.spyOn(window, "fetch").mockImplementation((input) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (url.includes("/shelves"))
      return Promise.resolve(jsonResponse({ items: SHELVES, next_cursor: null }));
    if (url.includes("/authors")) return Promise.resolve(jsonResponse({ items: [] }));
    return Promise.resolve(jsonResponse([]));
  });
}

function Providers({ children }: { children: ReactElement }): ReactElement {
  const [client] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: false } } }),
  );
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

/** The installation sort stack the harness inherits when no override is set. */
const DEFAULT_STACK: readonly SortLevelParam[] = [{ field: "created_at", desc: true }];

/** Observes every sort gesture the rail dispatches, newest last. */
let sortChanges: (readonly SortLevelParam[])[] = [];

/**
 * Stands in for the page's sort ownership: holds the override, resolves the
 * effective stack (override else the installation stack), and treats an
 * empty gesture as reset, exactly as `LibraryPage` does through the
 * preferences hook. Sort deliberately does not flow through the URL here,
 * because it has no URL form.
 */
function SortOwner({
  initialOverride,
  initialFilters,
}: {
  initialOverride: readonly SortLevelParam[] | null;
  initialFilters: Partial<FilterState>;
}) {
  const [filters, setFilters] = useState({ ...emptyFilterState(), ...initialFilters });
  const [override, setOverride] = useState<readonly SortLevelParam[] | null>(initialOverride);
  return (
    <>
      <FilterRail
        owner={{
          filters,
          view: undefined,
          commitAll: setFilters,
          clearAll: () => {
            setFilters(emptyFilterState());
          },
          setView: () => {},
        }}
        showContradiction
        seriesOptions={SERIES}
        sortLevels={override ?? DEFAULT_STACK}
        sortInherited={override === null}
        onSortChange={(next) => {
          sortChanges.push(next);
          setOverride(next.length === 0 ? null : next);
        }}
      />
      <output data-testid="filter-state">{JSON.stringify(filters)}</output>
    </>
  );
}

/**
 * The real adapter over a browser router, matching production: search-param
 * writes go through `history.replaceState`, so the assertions read the
 * document's own URL. A memory router cannot be used here, because it keeps
 * its location in memory where those writes never land.
 */
function renderRail(
  initialFilters: Partial<FilterState> = {},
  sortOverride: readonly SortLevelParam[] | null = null,
): void {
  window.history.replaceState(null, "", "/library");
  const router = createBrowserRouter([
    {
      path: "/library",
      element: (
        <>
          <Providers>
            <SortOwner initialOverride={sortOverride} initialFilters={initialFilters} />
          </Providers>
        </>
      ),
    },
  ]);
  render(<RouterProvider router={router} />);
}

function currentSearch(): URLSearchParams {
  const raw: unknown = JSON.parse(screen.getByTestId("filter-state").textContent);
  const observed = LibraryVisitSchema.parse({ filters: raw }).filters;
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(filterStateToParams(observed))) {
    if (Array.isArray(value)) for (const token of value) params.append(key, token);
    else params.set(key, String(value));
  }
  return params;
}

function sectionByTitle(title: string): HTMLElement {
  const rail = screen.getByRole("complementary", { name: "Filters" });
  const section = within(rail)
    .getAllByRole("button", { name: new RegExp(`^${title}(?:\\s*\\d+)?$`) })
    .map((node) => node.closest("section"))
    .find((candidate): candidate is HTMLElement => candidate !== null);
  if (section === undefined) throw new Error(`no section titled ${title}`);
  return section;
}

function disclosureOf(section: HTMLElement): HTMLElement {
  const control = section.querySelector<HTMLElement>("button[aria-expanded]");
  if (control === null) throw new Error("section has no disclosure control");
  return control;
}

/** Collapsed content is hidden from queries, so a test that works inside an
 *  inactive section expands it first, as a reader would. */
function openSection(title: string): HTMLElement {
  const section = sectionByTitle(title);
  const disclosure = disclosureOf(section);
  if (disclosure.getAttribute("aria-expanded") === "false") fireEvent.click(disclosure);
  return section;
}

beforeEach(() => {
  mockFetchByUrl();
  sortChanges = [];
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("FilterRail series facet", () => {
  test("selecting a series sets the series with single-select semantics", async () => {
    renderRail();
    openSection("Series");
    const user = userEvent.setup();
    await user.click(screen.getByRole("checkbox", { name: "Discworld" }));
    await waitFor(() => {
      expect(currentSearch().get("series")).toBe("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    });
  });

  test("selecting the active series again clears the filter", async () => {
    renderRail({ series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" });
    const user = userEvent.setup();
    const box = screen.getByRole("checkbox", { name: "Discworld" });
    expect(box).toBeChecked();
    await user.click(box);
    await waitFor(() => {
      expect(currentSearch().get("series")).toBeNull();
    });
  });

  test("section clear resets the series param and drops any cursor", async () => {
    renderRail({ series: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb" });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Clear Series filters" }));
    await waitFor(() => {
      const search = currentSearch();
      expect(search.get("series")).toBeNull();
      expect(search.get("cursor")).toBeNull();
    });
  });
});

describe("FilterRail shelf facet", () => {
  test("lists shelves from the API and picking one sets the shelf", async () => {
    renderRail({});
    openSection("Shelf");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("checkbox", { name: "Wishlist" }));
    await waitFor(() => {
      const search = currentSearch();
      expect(search.get("shelf")).toBe("cccccccc-cccc-4ccc-8ccc-cccccccccccc");
      expect(search.get("cursor")).toBeNull();
    });
  });

  test("Space re-picks an active shelf to clear its single value", async () => {
    renderRail({ shelf: "cccccccc-cccc-4ccc-8ccc-cccccccccccc" });
    const user = userEvent.setup();
    const shelf = await screen.findByRole("checkbox", { name: "Wishlist" });
    expect(shelf).toBeChecked();
    shelf.focus();
    await user.keyboard(" ");
    await waitFor(() => {
      expect(currentSearch().getAll("shelf")).toEqual([]);
    });
    expect(shelf).not.toBeChecked();
    expect(shelf).toHaveFocus();
  });

  test("section clear removes the shelf filter", async () => {
    renderRail({ shelf: "dddddddd-dddd-4ddd-8ddd-dddddddddddd" });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Clear Shelf filters" }));
    await waitFor(() => {
      expect(currentSearch().get("shelf")).toBeNull();
    });
  });
});

describe("FilterRail status section", () => {
  test("checking a status writes status_any immediately", async () => {
    renderRail();
    openSection("Status");
    const user = userEvent.setup();
    await user.click(screen.getByRole("checkbox", { name: "Reading" }));
    await waitFor(() => {
      expect(currentSearch().getAll("status_any")).toEqual(["reading"]);
    });
  });

  test("section clear drops status_any and status_none together", async () => {
    renderRail({ status: { any: ["reading"], none: ["abandoned"] } });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Clear Status filters" }));
    await waitFor(() => {
      const search = currentSearch();
      expect(search.getAll("status_any")).toEqual([]);
      expect(search.getAll("status_none")).toEqual([]);
    });
  });
});

describe("FilterRail typed sections", () => {
  test("title text commits title_contains on native completion", async () => {
    renderRail();
    const user = userEvent.setup();
    const title = openSection("Title");
    await user.type(within(title).getByRole("textbox", { name: "Filter value" }), "dune");

    await waitFor(() => {
      expect(currentSearch().get("title_contains")).toBe("dune");
    });
  });

  test("a multi-word title filter survives typing", async () => {
    // The committed value is trimmed. Rendering it straight back into the
    // box would delete the space the instant it is typed, and the next
    // character would land against the previous word.
    renderRail();
    const user = userEvent.setup();
    const title = openSection("Title");
    const input = within(title).getByRole("textbox", { name: "Filter value" });
    await user.type(input, "the sea");
    expect(input).toHaveValue("the sea");

    await waitFor(() => {
      expect(currentSearch().get("title_contains")).toBe("the sea");
    });
  });

  test("a section clear empties the box it cleared", async () => {
    renderRail({ title: { op: "contains", text: "the sea" } });
    const user = userEvent.setup();
    const title = sectionByTitle("Title");
    expect(within(title).getByRole("textbox", { name: "Filter value" })).toHaveValue("the sea");
    await user.click(screen.getByRole("button", { name: "Clear Title filters" }));
    await waitFor(() => {
      expect(within(title).getByRole("textbox", { name: "Filter value" })).toHaveValue("");
    });
  });
});

describe("FilterRail section state", () => {
  test("a section with active conditions mounts open, with a count badge", () => {
    renderRail({ pages: { gte: 300, lte: 900 } });
    const pages = sectionByTitle("Pages");
    expect(disclosureOf(pages)).toHaveAttribute("aria-expanded", "true");
    expect(within(pages).getByText("2")).toBeInTheDocument();
  });

  test("an inactive section mounts collapsed and shows no badge or clear control", () => {
    renderRail();
    const title = sectionByTitle("Title");
    expect(disclosureOf(title)).toHaveAttribute("aria-expanded", "false");
    expect(
      within(title).queryByRole("button", { name: "Clear Title filters" }),
    ).not.toBeInTheDocument();
  });

  test("the clear control is a sibling of the disclosure, never nested in it", () => {
    renderRail({ pages: { gte: 300 } });
    const pages = sectionByTitle("Pages");
    const clear = within(pages).getByRole("button", { name: "Clear Pages filters" });
    expect(disclosureOf(pages)).not.toContainElement(clear);
    expect(pages.querySelector("summary")).toBeNull();
  });

  test("Enter and Space toggle the disclosure from the keyboard", async () => {
    renderRail({ pages: { gte: 300 } });
    const user = userEvent.setup();
    const pages = sectionByTitle("Pages");
    const disclosure = disclosureOf(pages);
    const content = document.getElementById(disclosure.getAttribute("aria-controls") ?? "");
    disclosure.focus();
    await user.keyboard("{Enter}");
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    expect(content).toHaveAttribute("hidden", "until-found");
    await user.keyboard(" ");
    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    expect(content).not.toHaveAttribute("hidden");
  });

  test("a find-in-page match inside a collapsed section expands it", () => {
    renderRail();
    const title = sectionByTitle("Title");
    const disclosure = disclosureOf(title);
    const content = document.getElementById(disclosure.getAttribute("aria-controls") ?? "");
    if (content === null) throw new Error("disclosure controls no content");
    expect(content).toHaveAttribute("hidden", "until-found");
    fireEvent(content, new Event("beforematch"));
    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    expect(content).not.toHaveAttribute("hidden");
  });

  test("Tab moves from the disclosure to its clear control", async () => {
    renderRail({ pages: { gte: 300 } });
    const user = userEvent.setup();
    const pages = sectionByTitle("Pages");
    disclosureOf(pages).focus();
    await user.tab();
    expect(within(pages).getByRole("button", { name: "Clear Pages filters" })).toHaveFocus();
  });

  test("clearing from the keyboard leaves the disclosure state alone", async () => {
    renderRail({ pages: { gte: 300 } });
    const user = userEvent.setup();
    const pages = sectionByTitle("Pages");
    within(pages).getByRole("button", { name: "Clear Pages filters" }).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => {
      expect(currentSearch().get("pages_gte")).toBeNull();
    });
    expect(disclosureOf(pages)).toHaveAttribute("aria-expanded", "true");
  });

  test("clearing a collapsed section does not expand it", async () => {
    renderRail({ pages: { gte: 300 } });
    const user = userEvent.setup();
    const pages = sectionByTitle("Pages");
    await user.click(disclosureOf(pages));
    await user.click(within(pages).getByRole("button", { name: "Clear Pages filters" }));
    await waitFor(() => {
      expect(currentSearch().get("pages_gte")).toBeNull();
    });
    expect(disclosureOf(pages)).toHaveAttribute("aria-expanded", "false");
  });

  test("vocabulary sections badge the token count from every mode", () => {
    renderRail({ tags: { all: [], any: ["fantasy", "magic"], none: ["grimdark"] } });
    const tags = sectionByTitle("Tags");
    expect(disclosureOf(tags)).toHaveAttribute("aria-expanded", "true");
    expect(within(tags).getByText("3")).toBeInTheDocument();
  });
});

describe("FilterRail sort section", () => {
  test("adding a field appends a level to the effective stack", async () => {
    renderRail();
    openSection("Sort");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Add sort field/ }));
    await user.click(screen.getByRole("option", { name: "Title" }));
    // The inherited installation stack is what the reader sees, so adding a
    // field extends it: the whole effective stack materialises as their
    // override, not a single-level stack that silently dropped the default.
    await waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([
        { field: "created_at", desc: true },
        { field: "title", desc: false },
      ]);
    });
  });

  test("direction toggle, reorder, and remove dispatch the edited stack", async () => {
    renderRail({}, [
      { field: "title", desc: false },
      { field: "created_at", desc: true },
    ]);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Change Title sort direction/ }));
    await waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([
        { field: "title", desc: true },
        { field: "created_at", desc: true },
      ]);
    });
    await user.click(screen.getByRole("button", { name: "Move Added earlier in sort priority" }));
    await waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([
        { field: "created_at", desc: true },
        { field: "title", desc: true },
      ]);
    });
    await user.click(screen.getByRole("button", { name: "Remove Added from sort" }));
    await waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([{ field: "title", desc: true }]);
    });
  });

  test("reset dispatches an empty stack and the section returns to the inherited order", async () => {
    renderRail({}, [{ field: "pages", desc: true }]);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Reset sort to the installation order" }));
    await waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([]);
    });
    // The stack visibly becomes the installation order: never empty,
    // because the library always has a total order.
    const sort = sectionByTitle("Sort");
    expect(within(sort).getByText("Added")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Reset sort to the installation order" }),
    ).not.toBeInTheDocument();
  });

  test("removing the last level is the reset gesture, not an unsorted state", async () => {
    renderRail({}, [{ field: "pages", desc: true }]);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Remove Pages from sort" }));
    await waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([]);
    });
    const sort = sectionByTitle("Sort");
    expect(within(sort).getByText("Added")).toBeInTheDocument();
  });

  test("an inherited stack reads as at rest: no badge, no reset control", () => {
    renderRail();
    const sort = sectionByTitle("Sort");
    // The header row is bare: no count badge, no reset control.
    expect(disclosureOf(sort).parentElement?.textContent).toBe("Sort");
    // The effective stack still renders: the library is never unsorted.
    expect(within(sort).getByText("Added")).toBeInTheDocument();
  });

  test("the last inherited level's remove control is disabled, never a dead press", () => {
    renderRail();
    openSection("Sort");
    // Removing it would dispatch a reset to the state already showing, a
    // press that visibly does nothing; direction and reorder stay live.
    expect(screen.getByRole("button", { name: "Remove Added from sort" })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Change Added sort direction/ })).toBeEnabled();
  });

  test("an override's sole level keeps a working remove: it is the reset gesture", () => {
    renderRail({}, [{ field: "pages", desc: true }]);
    expect(screen.getByRole("button", { name: "Remove Pages from sort" })).toBeEnabled();
  });

  test("editing an inherited level materialises it as the reader's override", async () => {
    renderRail();
    openSection("Sort");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Change Added sort direction/ }));
    await waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([{ field: "created_at", desc: false }]);
    });
  });

  test("the rail hosts no live region of its own; announcements are page-level", () => {
    renderRail({}, [{ field: "pages", desc: true }]);
    const rail = screen.getByRole("complementary", { name: "Filters" });
    expect(rail.querySelector('[aria-live="polite"], [aria-live="assertive"]')).toBeNull();
  });

  test("the add-field picker hides fields already in the stack and caps the stack", () => {
    renderRail({}, [
      { field: "title", desc: false },
      { field: "created_at", desc: true },
      { field: "pages", desc: false },
    ]);
    expect(screen.queryByRole("combobox", { name: "Add sort field" })).not.toBeInTheDocument();
  });
});

describe("FilterRail vocabulary sections", () => {
  test("clearing a vocabulary section drops all three mode params", async () => {
    renderRail({
      authors: { all: ["eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"], any: ["a-2"], none: ["a-3"] },
    });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Clear Authors filters" }));
    const search = currentSearch();
    expect(search.getAll("author")).toEqual([]);
    expect(search.getAll("author_any")).toEqual([]);
    expect(search.getAll("author_none")).toEqual([]);
  });
});
