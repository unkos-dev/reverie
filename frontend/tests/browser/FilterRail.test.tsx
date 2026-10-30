import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { emptyFilterState, type FilterState } from "@/routes/library-params";
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
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
  mockFetchByUrl();
  sortChanges = [];
});

afterEach(() => {
  vi.restoreAllMocks();
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
});
