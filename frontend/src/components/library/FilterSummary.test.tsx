import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement } from "react";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";

import { emptyFilterState, type FilterState } from "@/routes/library-params";

import { buildFilterSummary, shortId } from "./filter-summary";
import { FilterSummary } from "./FilterSummary";

const RESOLVERS = {
  authorLabel: (id: string): string => (id === "a-1" ? "Ursula K. Le Guin" : id),
  shelfName: (id: string): string => (id === "shelf-1" ? "Wishlist" : shortId(id)),
  seriesName: (id: string): string => (id === "s-1" ? "Discworld" : shortId(id)),
};

function stateWith(patch: Partial<FilterState>): FilterState {
  return { ...emptyFilterState(), ...patch };
}

describe("buildFilterSummary", () => {
  test("an empty state produces no segments", () => {
    expect(buildFilterSummary(emptyFilterState(), RESOLVERS)).toEqual([]);
  });

  test("covers the full grammar in a fixed order", () => {
    const state = stateWith({
      q: "dune",
      shelf: "shelf-1",
      series: "s-1",
      authors: { all: [], any: ["a-1"], none: [] },
      tags: { all: ["fantasy"], any: ["magic"], none: ["grimdark"] },
      status: { any: ["reading"], none: [] },
      title: { contains: "sea" },
      pages: { gte: 300 },
      rating: { lte: 4 },
      addedAfter: "2026-01-01",
    });
    expect(buildFilterSummary(state, RESOLVERS).map((segment) => segment.text)).toEqual([
      'Search "dune"',
      "Shelf: Wishlist",
      "Series: Discworld",
      "Author: Ursula K. Le Guin",
      "Tags (3)",
      "Status: Reading",
      'Title contains "sea"',
      "Pages ≥ 300",
      "Rating ≤ 4",
      "Added after 2026-01-01",
    ]);
  });

  test("a multi-token vocabulary family summarises as a count, a single token as its name", () => {
    const many = stateWith({ authors: { all: ["a-1"], any: ["a-2"], none: [] } });
    expect(buildFilterSummary(many, RESOLVERS).map((segment) => segment.text)).toEqual([
      "Authors (2)",
    ]);
    const one = stateWith({ authors: { all: [], any: ["a-1"], none: [] } });
    expect(buildFilterSummary(one, RESOLVERS).map((segment) => segment.text)).toEqual([
      "Author: Ursula K. Le Guin",
    ]);
  });

  test("unresolvable shelf and series ids degrade to a shortened id", () => {
    const state = stateWith({
      shelf: "0f0f0f0f-1111-2222-3333-444444444444",
      series: "9e9e9e9e-1111-2222-3333-444444444444",
    });
    expect(buildFilterSummary(state, RESOLVERS).map((segment) => segment.text)).toEqual([
      "Shelf: 0f0f0f0f…",
      "Series: 9e9e9e9e…",
    ]);
  });

  test("text operators and range bounds each contribute a segment", () => {
    const state = stateWith({
      title: { contains: "sea", ne: "sky" },
      pages: { gte: 100, lte: 900, empty: false },
    });
    expect(buildFilterSummary(state, RESOLVERS).map((segment) => segment.text)).toEqual([
      'Title contains "sea"',
      'Title is not "sky"',
      "Pages ≥ 100",
      "Pages ≤ 900",
      "Pages is set",
    ]);
  });
});

function Harness({
  filters,
  railExpanded = true,
  onToggleRail = () => undefined,
}: {
  filters: FilterState;
  railExpanded?: boolean;
  onToggleRail?: () => void;
}): ReactElement {
  const [client] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: false } } }),
  );
  return (
    <QueryClientProvider client={client}>
      <FilterSummary
        filters={filters}
        seriesNames={new Map([["s-1", "Discworld"]])}
        railExpanded={railExpanded}
        onToggleRail={onToggleRail}
      />
    </QueryClientProvider>
  );
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("FilterSummary", () => {
  test("renders the readout and a badge with the segment count", () => {
    render(<Harness filters={stateWith({ series: "s-1", pages: { gte: 300 } })} />);
    expect(screen.getByText("Series: Discworld · Pages ≥ 300")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Filters/ })).toHaveTextContent("2");
  });

  test("no active filters renders the toggle alone, without a badge", () => {
    render(<Harness filters={emptyFilterState()} />);
    const toggle = screen.getByRole("button", { name: "Filters" });
    expect(toggle).not.toHaveTextContent(/\d/);
    expect(screen.queryByText(/·/)).not.toBeInTheDocument();
  });

  test("the toggle reflects rail visibility and reports clicks", async () => {
    const onToggleRail = vi.fn();
    render(
      <Harness filters={emptyFilterState()} railExpanded={false} onToggleRail={onToggleRail} />,
    );
    const toggle = screen.getByRole("button", { name: "Filters" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await userEvent.setup().click(toggle);
    expect(onToggleRail).toHaveBeenCalledTimes(1);
  });

  test("the toggle is the only interactive element", () => {
    render(<Harness filters={stateWith({ q: "dune", series: "s-1" })} />);
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });
});
