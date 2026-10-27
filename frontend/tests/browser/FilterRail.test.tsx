import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import {
  emptyFilterState,
  filterStateToParams,
  LibraryVisitSchema,
  type FilterState,
} from "@/routes/library-params";
import { useState, type ReactElement } from "react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import type { SortLevelParam } from "@/api";

import { FilterRail } from "@/components/shell/FilterRail";

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
    let url: string;
    if (typeof input === "string") {
      url = input;
    } else if (input instanceof URL) {
      url = input.href;
    } else {
      url = input.url;
    }
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

const DEFAULT_STACK: readonly SortLevelParam[] = [{ field: "created_at", desc: true }];

let sortChanges: (readonly SortLevelParam[])[] = [];

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

async function renderRail(
  initialFilters: Partial<FilterState> = {},
  sortOverride: readonly SortLevelParam[] | null = null,
): Promise<void> {
  const router = createMemoryRouter(
    [
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
    ],
    { initialEntries: ["/library"] },
  );
  await render(<RouterProvider router={router} />);
}

function currentSearch(): URLSearchParams {
  const raw: unknown = JSON.parse(page.getByTestId("filter-state").element().textContent);
  const observed = LibraryVisitSchema.parse({ filters: raw }).filters;
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(filterStateToParams(observed))) {
    if (Array.isArray(value)) for (const token of value) params.append(key, token);
    else params.set(key, String(value));
  }
  return params;
}

function sectionByTitle(title: string): HTMLElement {
  const rail = page.getByRole("complementary", { name: "Filters" });
  const section = rail
    .getByRole("button", { name: new RegExp(`^${title}(?:\\s*\\d+)?$`) })
    .elements()
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

async function openSection(title: string): Promise<HTMLElement> {
  const section = sectionByTitle(title);
  const disclosure = disclosureOf(section);
  if (disclosure.getAttribute("aria-expanded") === "false") await userEvent.click(disclosure);
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
    await renderRail();
    await openSection("Series");
    await page.screenshot({ path: "test-results/browser/library-filters.png" });
    const user = userEvent;
    await user.click(page.getByRole("checkbox", { name: "Discworld" }));
    await vi.waitFor(() => {
      expect(currentSearch().get("series")).toBe("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    });
  });

  test("selecting the active series again clears the filter", async () => {
    await renderRail({ series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" });
    const user = userEvent;
    const box = page.getByRole("checkbox", { name: "Discworld" });
    await expect.element(box).toBeChecked();
    await user.click(box);
    await vi.waitFor(() => {
      expect(currentSearch().get("series")).toBeNull();
    });
  });

  test("section clear resets the series param and drops any cursor", async () => {
    await renderRail({ series: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb" });
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Clear Series filters" }));
    await vi.waitFor(() => {
      const search = currentSearch();
      expect(search.get("series")).toBeNull();
      expect(search.get("cursor")).toBeNull();
    });
  });
});

describe("FilterRail shelf facet", () => {
  test("lists shelves from the API and picking one sets the shelf", async () => {
    await renderRail({});
    await openSection("Shelf");
    const user = userEvent;
    await user.click(page.getByRole("checkbox", { name: "Wishlist" }));
    await vi.waitFor(() => {
      const search = currentSearch();
      expect(search.get("shelf")).toBe("cccccccc-cccc-4ccc-8ccc-cccccccccccc");
      expect(search.get("cursor")).toBeNull();
    });
  });

  test("Space re-picks an active shelf to clear its single value", async () => {
    await renderRail({ shelf: "cccccccc-cccc-4ccc-8ccc-cccccccccccc" });
    const user = userEvent;
    const shelf = page.getByRole("checkbox", { name: "Wishlist" });
    await expect.element(shelf).toBeChecked();
    shelf.element().focus();
    await user.keyboard(" ");
    await vi.waitFor(() => {
      expect(currentSearch().getAll("shelf")).toEqual([]);
    });
    await expect.element(shelf).not.toBeChecked();
    await expect.element(shelf).toHaveFocus();
  });

  test("section clear removes the shelf filter", async () => {
    await renderRail({ shelf: "dddddddd-dddd-4ddd-8ddd-dddddddddddd" });
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Clear Shelf filters" }));
    await vi.waitFor(() => {
      expect(currentSearch().get("shelf")).toBeNull();
    });
  });
});

describe("FilterRail status section", () => {
  test("checking a status writes status_any immediately", async () => {
    await renderRail();
    await openSection("Status");
    const user = userEvent;
    await user.click(page.getByRole("checkbox", { name: "Reading" }));
    await vi.waitFor(() => {
      expect(currentSearch().getAll("status_any")).toEqual(["reading"]);
    });
  });

  test("section clear drops status_any and status_none together", async () => {
    await renderRail({ status: { any: ["reading"], none: ["abandoned"] } });
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Clear Status filters" }));
    await vi.waitFor(() => {
      const search = currentSearch();
      expect(search.getAll("status_any")).toEqual([]);
      expect(search.getAll("status_none")).toEqual([]);
    });
  });
});

describe("FilterRail typed sections", () => {
  test("title text commits title_contains on native completion", async () => {
    await renderRail();
    const user = userEvent;
    const title = await openSection("Title");
    await user.type(
      page.elementLocator(title).getByRole("textbox", { name: "Filter value" }),
      "dune",
    );

    await vi.waitFor(() => {
      expect(currentSearch().get("title_contains")).toBe("dune");
    });
  });

  test("a multi-word title filter survives typing", async () => {
    await renderRail();
    const user = userEvent;
    const title = await openSection("Title");
    const input = page.elementLocator(title).getByRole("textbox", { name: "Filter value" });
    await user.type(input, "the sea");
    await expect.element(input).toHaveValue("the sea");

    await vi.waitFor(() => {
      expect(currentSearch().get("title_contains")).toBe("the sea");
    });
  });

  test("a section clear empties the box it cleared", async () => {
    await renderRail({ title: { op: "contains", text: "the sea" } });
    const user = userEvent;
    const title = sectionByTitle("Title");
    await expect
      .element(page.elementLocator(title).getByRole("textbox", { name: "Filter value" }))
      .toHaveValue("the sea");
    await user.click(page.getByRole("button", { name: "Clear Title filters" }));
    await expect
      .element(page.elementLocator(title).getByRole("textbox", { name: "Filter value" }))
      .toHaveValue("");
  });
});

describe("FilterRail section state", () => {
  test("a section with active conditions mounts open, with a count badge", async () => {
    await renderRail({ pages: { gte: 300, lte: 900 } });
    const pages = sectionByTitle("Pages");
    await expect.element(disclosureOf(pages)).toHaveAttribute("aria-expanded", "true");
    await expect.element(page.elementLocator(pages).getByText("2")).toBeInTheDocument();
  });

  test("an inactive section mounts collapsed and shows no badge or clear control", async () => {
    await renderRail();
    const title = sectionByTitle("Title");
    await expect.element(disclosureOf(title)).toHaveAttribute("aria-expanded", "false");
    await expect
      .element(page.elementLocator(title).getByRole("button", { name: "Clear Title filters" }))
      .not.toBeInTheDocument();
  });

  test("the clear control is a sibling of the disclosure, never nested in it", async () => {
    await renderRail({ pages: { gte: 300 } });
    const pages = sectionByTitle("Pages");
    const clear = page.elementLocator(pages).getByRole("button", { name: "Clear Pages filters" });
    await expect.element(disclosureOf(pages)).not.toContainElement(clear.element());
    expect(pages.querySelector("summary")).toBeNull();
  });

  test("Enter and Space toggle the disclosure from the keyboard", async () => {
    await renderRail({ pages: { gte: 300 } });
    const user = userEvent;
    const pages = sectionByTitle("Pages");
    const disclosure = disclosureOf(pages);
    const content = document.getElementById(disclosure.getAttribute("aria-controls") ?? "");
    disclosure.focus();
    await user.keyboard("{Enter}");
    await expect.element(disclosure).toHaveAttribute("aria-expanded", "false");
    await expect.element(content).toHaveAttribute("hidden", "until-found");
    await user.keyboard(" ");
    await expect.element(disclosure).toHaveAttribute("aria-expanded", "true");
    await expect.element(content).not.toHaveAttribute("hidden");
  });

  test("a find-in-page match inside a collapsed section expands it", async () => {
    await renderRail();
    const title = sectionByTitle("Title");
    const disclosure = disclosureOf(title);
    const content = document.getElementById(disclosure.getAttribute("aria-controls") ?? "");
    if (content === null) throw new Error("disclosure controls no content");
    await expect.element(content).toHaveAttribute("hidden", "until-found");
    content.dispatchEvent(new Event("beforematch"));
    await expect.element(disclosure).toHaveAttribute("aria-expanded", "true");
    await expect.element(content).not.toHaveAttribute("hidden");
  });

  test("Tab moves from the disclosure to its clear control", async () => {
    await renderRail({ pages: { gte: 300 } });
    const user = userEvent;
    const pages = sectionByTitle("Pages");
    disclosureOf(pages).focus();
    await user.tab();
    await expect
      .element(page.elementLocator(pages).getByRole("button", { name: "Clear Pages filters" }))
      .toHaveFocus();
  });

  test("clearing from the keyboard leaves the disclosure state alone", async () => {
    await renderRail({ pages: { gte: 300 } });
    const user = userEvent;
    const pages = sectionByTitle("Pages");
    page
      .elementLocator(pages)
      .getByRole("button", { name: "Clear Pages filters" })
      .element()
      .focus();
    await user.keyboard("{Enter}");
    await vi.waitFor(() => {
      expect(currentSearch().get("pages_gte")).toBeNull();
    });
    await expect.element(disclosureOf(pages)).toHaveAttribute("aria-expanded", "true");
  });

  test("clearing a collapsed section does not expand it", async () => {
    await renderRail({ pages: { gte: 300 } });
    const user = userEvent;
    const pages = sectionByTitle("Pages");
    await user.click(disclosureOf(pages));
    await user.click(
      page.elementLocator(pages).getByRole("button", { name: "Clear Pages filters" }),
    );
    await vi.waitFor(() => {
      expect(currentSearch().get("pages_gte")).toBeNull();
    });
    await expect.element(disclosureOf(pages)).toHaveAttribute("aria-expanded", "false");
  });

  test("vocabulary sections badge the token count from every mode", async () => {
    await renderRail({ tags: { all: [], any: ["fantasy", "magic"], none: ["grimdark"] } });
    const tags = sectionByTitle("Tags");
    await expect.element(disclosureOf(tags)).toHaveAttribute("aria-expanded", "true");
    await expect.element(page.elementLocator(tags).getByText("3")).toBeInTheDocument();
  });
});

describe("FilterRail sort section", () => {
  test("adding a field appends a level to the effective stack", async () => {
    await renderRail();
    await openSection("Sort");
    const user = userEvent;
    await user.click(page.getByRole("button", { name: /Add sort field/ }));
    await user.click(page.getByRole("option", { name: "Title" }));

    await vi.waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([
        { field: "created_at", desc: true },
        { field: "title", desc: false },
      ]);
    });
  });

  test("direction toggle, reorder, and remove dispatch the edited stack", async () => {
    await renderRail({}, [
      { field: "title", desc: false },
      { field: "created_at", desc: true },
    ]);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: /Change Title sort direction/ }));
    await vi.waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([
        { field: "title", desc: true },
        { field: "created_at", desc: true },
      ]);
    });
    await user.click(page.getByRole("button", { name: "Move Added earlier in sort priority" }));
    await vi.waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([
        { field: "created_at", desc: true },
        { field: "title", desc: true },
      ]);
    });
    await user.click(page.getByRole("button", { name: "Remove Added from sort" }));
    await vi.waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([{ field: "title", desc: true }]);
    });
  });

  test("reset dispatches an empty stack and the section returns to the inherited order", async () => {
    await renderRail({}, [{ field: "pages", desc: true }]);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Reset sort to the installation order" }));
    await vi.waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([]);
    });

    const sort = sectionByTitle("Sort");
    await expect.element(page.elementLocator(sort).getByText("Added")).toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: "Reset sort to the installation order" }))
      .not.toBeInTheDocument();
  });

  test("removing the last level is the reset gesture, not an unsorted state", async () => {
    await renderRail({}, [{ field: "pages", desc: true }]);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Remove Pages from sort" }));
    await vi.waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([]);
    });
    const sort = sectionByTitle("Sort");
    await expect.element(page.elementLocator(sort).getByText("Added")).toBeInTheDocument();
  });

  test("an inherited stack reads as at rest: no badge, no reset control", async () => {
    await renderRail();
    const sort = sectionByTitle("Sort");

    expect(disclosureOf(sort).parentElement?.textContent).toBe("Sort");

    await expect.element(page.elementLocator(sort).getByText("Added")).toBeInTheDocument();
  });

  test("the last inherited level's remove control is disabled, never a dead press", async () => {
    await renderRail();
    await openSection("Sort");

    await expect
      .element(page.getByRole("button", { name: "Remove Added from sort" }))
      .toBeDisabled();
    await expect
      .element(page.getByRole("button", { name: /Change Added sort direction/ }))
      .toBeEnabled();
  });

  test("an override's sole level keeps a working remove: it is the reset gesture", async () => {
    await renderRail({}, [{ field: "pages", desc: true }]);
    await expect
      .element(page.getByRole("button", { name: "Remove Pages from sort" }))
      .toBeEnabled();
  });

  test("editing an inherited level materialises it as the reader's override", async () => {
    await renderRail();
    await openSection("Sort");
    const user = userEvent;
    await user.click(page.getByRole("button", { name: /Change Added sort direction/ }));
    await vi.waitFor(() => {
      expect(sortChanges.at(-1)).toEqual([{ field: "created_at", desc: false }]);
    });
  });

  test("the rail hosts no live region of its own; announcements are page-level", async () => {
    await renderRail({}, [{ field: "pages", desc: true }]);
    const rail = page.getByRole("complementary", { name: "Filters" });
    expect(
      rail.element().querySelector('[aria-live="polite"], [aria-live="assertive"]'),
    ).toBeNull();
  });

  test("the add-field picker hides fields already in the stack and caps the stack", async () => {
    await renderRail({}, [
      { field: "title", desc: false },
      { field: "created_at", desc: true },
      { field: "pages", desc: false },
    ]);
    await expect
      .element(page.getByRole("combobox", { name: "Add sort field" }))
      .not.toBeInTheDocument();
  });
});

describe("FilterRail vocabulary sections", () => {
  test("clearing a vocabulary section drops all three mode params", async () => {
    await renderRail({
      authors: { all: ["eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"], any: ["a-2"], none: ["a-3"] },
    });
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Clear Authors filters" }));
    const search = currentSearch();
    expect(search.getAll("author")).toEqual([]);
    expect(search.getAll("author_any")).toEqual([]);
    expect(search.getAll("author_none")).toEqual([]);
  });
});
