import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __resetCsrfTokenForTesting } from "@/api/csrf";
import { emptyFilterState, type FilterState } from "@/routes/library-params";

import { FilterBar } from "./FilterBar";

const LE_GUIN = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function Harness({ initial = emptyFilterState() }: { initial?: FilterState }): ReactElement {
  const [client] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: false } } }),
  );
  const [filters, setFilters] = useState<FilterState>(initial);
  return (
    <QueryClientProvider client={client}>
      <FilterBar filters={filters} onFiltersChange={setFilters} />
    </QueryClientProvider>
  );
}

beforeEach(() => {
  __resetCsrfTokenForTesting();
  vi.restoreAllMocks();
});

afterEach(() => {
  __resetCsrfTokenForTesting();
});

describe("FilterBar quick search", () => {
  test("commits a 2+ character query as a chip after the debounce", async () => {
    render(<Harness />);
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Quick search"), "war");

    expect(await screen.findByText("Search: war")).toBeInTheDocument();
  });

  test("does not commit a single-character query", async () => {
    render(<Harness />);
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Quick search"), "w");
    await new Promise((resolve) => setTimeout(resolve, 400));

    expect(screen.queryByText(/^Search:/)).not.toBeInTheDocument();
  });
});

describe("FilterBar builder", () => {
  test("builds a numeric condition and shows its chip", async () => {
    render(<Harness />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /add filter/i }));
    await user.click(screen.getByRole("button", { name: "Pages" }));
    fireEvent.change(screen.getByLabelText("Min"), { target: { value: "500" } });
    await user.click(screen.getByRole("button", { name: /apply/i }));

    expect(await screen.findByText("Pages ≥ 500")).toBeInTheDocument();
  });

  test("builds a status condition including the Unread pseudo-value", async () => {
    render(<Harness />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /add filter/i }));
    await user.click(screen.getByRole("button", { name: "Status" }));
    await user.click(screen.getByLabelText("Unread"));
    await user.click(screen.getByRole("button", { name: /apply/i }));

    expect(await screen.findByText("Status: Unread")).toBeInTheDocument();
  });
});

describe("FilterBar chips", () => {
  test("removing a chip drops exactly its condition", async () => {
    render(
      <Harness
        initial={{
          ...emptyFilterState(),
          pages: { gte: 500 },
          rating: { gte: 3 },
        }}
      />,
    );
    const user = userEvent.setup();

    expect(screen.getByText("Pages ≥ 500")).toBeInTheDocument();
    await user.click(screen.getByText("Pages ≥ 500"));

    expect(screen.queryByText("Pages ≥ 500")).not.toBeInTheDocument();
    expect(screen.getByText("Rating ≥ 3")).toBeInTheDocument();
  });

  test("clear filters empties every condition", async () => {
    render(<Harness initial={{ ...emptyFilterState(), q: "war", pages: { gte: 500 } }} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /clear filters/i }));

    expect(screen.queryByText("Search: war")).not.toBeInTheDocument();
    expect(screen.queryByText("Pages ≥ 500")).not.toBeInTheDocument();
  });
});

describe("FilterBar author hydration", () => {
  test("resolves an author id to a name chip", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      jsonResponse({ items: [{ id: LE_GUIN, value: "Ursula K. Le Guin" }] }),
    );
    render(
      <Harness
        initial={{
          ...emptyFilterState(),
          authors: { all: [], any: [LE_GUIN], none: [] },
        }}
      />,
    );

    expect(await screen.findByText(/Author any of: Ursula K\. Le Guin/)).toBeInTheDocument();
  });

  test("labels an unresolved author id as unknown", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(jsonResponse({ items: [] }));
    render(
      <Harness
        initial={{
          ...emptyFilterState(),
          authors: { all: [], any: [LE_GUIN], none: [] },
        }}
      />,
    );

    expect(await screen.findByText(/Author any of: Unknown author/)).toBeInTheDocument();
  });
});
