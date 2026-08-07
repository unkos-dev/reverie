import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NuqsAdapter } from "nuqs/adapters/react-router/v8";
import { useState, type ReactElement } from "react";
import { createBrowserRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { FilterRail } from "./FilterRail";

const SERIES = [
  { id: "s-1", name: "Discworld" },
  { id: "s-2", name: "Culture" },
];

const SHELVES = [
  {
    id: "shelf-1",
    name: "Wishlist",
    is_system: false,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    item_count: 3,
  },
  {
    id: "shelf-2",
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

/**
 * The real adapter over a browser router, matching production: search-param
 * writes go through `history.replaceState`, so the assertions read the
 * document's own URL. A memory router cannot be used here, because it keeps
 * its location in memory where those writes never land.
 */
function renderRail(initialEntry = "/library"): void {
  window.history.replaceState(null, "", initialEntry);
  const router = createBrowserRouter([
    {
      path: "/library",
      element: (
        <NuqsAdapter>
          <Providers>
            <FilterRail seriesOptions={SERIES} />
          </Providers>
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

function sectionByTitle(title: string): HTMLElement {
  const rail = screen.getByRole("complementary", { name: "Filters" });
  const details = within(rail)
    .getAllByText(title)
    .map((node) => node.closest("details"))
    .find((candidate): candidate is HTMLDetailsElement => candidate !== null);
  if (details === undefined) throw new Error(`no section titled ${title}`);
  return details;
}

beforeEach(() => {
  mockFetchByUrl();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("FilterRail series facet", () => {
  test("selecting a series sets ?series= with single-select semantics", async () => {
    renderRail();
    const user = userEvent.setup();
    await user.click(screen.getByRole("checkbox", { name: "Discworld" }));
    await waitFor(() => {
      expect(currentSearch().get("series")).toBe("s-1");
    });
  });

  test("selecting the active series again clears the filter", async () => {
    renderRail("/library?series=s-1");
    const user = userEvent.setup();
    const box = screen.getByRole("checkbox", { name: "Discworld" });
    expect(box).toBeChecked();
    await user.click(box);
    await waitFor(() => {
      expect(currentSearch().get("series")).toBeNull();
    });
  });

  test("section clear resets the series param and drops any cursor", async () => {
    renderRail("/library?series=s-2&cursor=abc");
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
  test("lists shelves from the API and picking one writes ?shelf=", async () => {
    renderRail("/library?cursor=abc");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("checkbox", { name: "Wishlist" }));
    await waitFor(() => {
      const search = currentSearch();
      expect(search.get("shelf")).toBe("shelf-1");
      expect(search.get("cursor")).toBeNull();
    });
  });

  test("section clear removes the shelf filter", async () => {
    renderRail("/library?shelf=shelf-2");
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
    const user = userEvent.setup();
    await user.click(screen.getByRole("checkbox", { name: "Reading" }));
    await waitFor(() => {
      expect(currentSearch().getAll("status_any")).toEqual(["reading"]);
    });
  });

  test("section clear drops status_any and status_none together", async () => {
    renderRail("/library?status_any=reading&status_none=abandoned");
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
  test("title text commits title_contains after the debounce", async () => {
    renderRail();
    const user = userEvent.setup();
    const title = sectionByTitle("Title");
    await user.type(within(title).getByRole("textbox", { name: "Filter value" }), "dune");
    await debounceSettled();
    await waitFor(() => {
      expect(currentSearch().get("title_contains")).toBe("dune");
    });
  });

  test("pages min commits pages_gte after the debounce", async () => {
    renderRail();
    const user = userEvent.setup();
    const pages = sectionByTitle("Pages");
    await user.type(within(pages).getByRole("spinbutton", { name: "Min" }), "300");
    await debounceSettled();
    await waitFor(() => {
      expect(currentSearch().get("pages_gte")).toBe("300");
    });
  });

  test("a debounced commit in one section preserves another section's pending draft", async () => {
    renderRail();
    const user = userEvent.setup();
    const pages = sectionByTitle("Pages");
    await user.type(within(pages).getByRole("spinbutton", { name: "Min" }), "300");
    const title = sectionByTitle("Title");
    const titleInput = within(title).getByRole("textbox", { name: "Filter value" });
    await user.type(titleInput, "sea");
    await debounceSettled();
    await waitFor(() => {
      const search = currentSearch();
      expect(search.get("pages_gte")).toBe("300");
      expect(search.get("title_contains")).toBe("sea");
    });
    expect(titleInput).toHaveValue("sea");
  });

  test("an immediate commit elsewhere preserves a pending typed draft", async () => {
    renderRail();
    const user = userEvent.setup();
    const title = sectionByTitle("Title");
    const titleInput = within(title).getByRole("textbox", { name: "Filter value" });
    await user.type(titleInput, "sea");
    await user.click(screen.getByRole("checkbox", { name: "Discworld" }));
    await debounceSettled();
    await waitFor(() => {
      const search = currentSearch();
      expect(search.get("series")).toBe("s-1");
      expect(search.get("title_contains")).toBe("sea");
    });
    expect(titleInput).toHaveValue("sea");
  });

  test("section clear during a pending draft does not resurrect the value", async () => {
    renderRail("/library?pages_lte=900");
    const user = userEvent.setup();
    const pages = sectionByTitle("Pages");
    const min = within(pages).getByRole("spinbutton", { name: "Min" });
    await user.type(min, "300");
    await user.click(within(pages).getByRole("button", { name: "Clear Pages filters" }));
    await debounceSettled();
    await waitFor(() => {
      const search = currentSearch();
      expect(search.get("pages_gte")).toBeNull();
      expect(search.get("pages_lte")).toBeNull();
    });
    expect(min).toHaveValue(null);
  });
});

describe("FilterRail section state", () => {
  test("a section with active conditions mounts open, with a count badge", () => {
    renderRail("/library?pages_gte=300&pages_lte=900");
    const pages = sectionByTitle("Pages");
    expect(pages).toHaveAttribute("open");
    expect(within(pages).getByText("2")).toBeInTheDocument();
  });

  test("an inactive section mounts collapsed and shows no badge or clear control", () => {
    renderRail();
    const title = sectionByTitle("Title");
    expect(title).not.toHaveAttribute("open");
    expect(
      within(title).queryByRole("button", { name: "Clear Title filters" }),
    ).not.toBeInTheDocument();
  });

  test("vocabulary sections badge the token count from every mode", () => {
    renderRail("/library?tag_any=fantasy&tag_any=magic&tag_none=grimdark");
    const tags = sectionByTitle("Tags");
    expect(tags).toHaveAttribute("open");
    expect(within(tags).getByText("3")).toBeInTheDocument();
  });
});

describe("FilterRail sort section", () => {
  test("adding a field appends a sort level", async () => {
    renderRail();
    const user = userEvent.setup();
    await user.click(screen.getByRole("combobox", { name: "Add sort field" }));
    await user.click(screen.getByRole("option", { name: "Title" }));
    await waitFor(() => {
      expect(currentSearch().get("sort")).toBe("title");
    });
  });

  test("direction toggle, reorder, and remove rewrite ?sort=", async () => {
    renderRail("/library?sort=title,-created_at");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Change Title sort direction/ }));
    await waitFor(() => {
      expect(currentSearch().get("sort")).toBe("-title,-created_at");
    });
    await user.click(screen.getByRole("button", { name: "Move Added earlier in sort priority" }));
    await waitFor(() => {
      expect(currentSearch().get("sort")).toBe("-created_at,-title");
    });
    await user.click(screen.getByRole("button", { name: "Remove Added from sort" }));
    await waitFor(() => {
      expect(currentSearch().get("sort")).toBe("-title");
    });
  });

  test("sort clear empties the stack and drops the cursor", async () => {
    renderRail("/library?sort=title,-created_at&cursor=abc");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Clear Sort filters" }));
    await waitFor(() => {
      const search = currentSearch();
      expect(search.get("sort")).toBeNull();
      expect(search.get("cursor")).toBeNull();
    });
  });

  test("the rail hosts no live region of its own; announcements are page-level", () => {
    renderRail("/library?sort=-pages");
    const rail = screen.getByRole("complementary", { name: "Filters" });
    expect(rail.querySelector("[aria-live]")).toBeNull();
  });

  test("the add-field picker hides fields already in the stack and caps the stack", () => {
    renderRail("/library?sort=title,-created_at,pages");
    expect(screen.queryByRole("combobox", { name: "Add sort field" })).not.toBeInTheDocument();
  });
});

describe("FilterRail vocabulary sections", () => {
  test("clearing a vocabulary section drops all three mode params", async () => {
    renderRail("/library?author=a-1&author_any=a-2&author_none=a-3");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Clear Authors filters" }));
    const search = currentSearch();
    expect(search.getAll("author")).toEqual([]);
    expect(search.getAll("author_any")).toEqual([]);
    expect(search.getAll("author_none")).toEqual([]);
  });
});
