import { cleanup, act, fireEvent, render, screen } from "@testing-library/react";
import { useState, type ReactElement } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, test, vi } from "vite-plus/test";
import { emptyFilterState } from "@/routes/library-params";
import { useLibraryFilters } from "./use-library-filters";

let entry: Record<string, unknown>;
function installNavigation(state: Record<string, unknown> = {}): void {
  entry = state;
  const events = new EventTarget();
  vi.stubGlobal("navigation", {
    currentEntry: { key: "visit", getState: () => entry },
    updateCurrentEntry: ({ state: next }: { state: Record<string, unknown> }) => {
      entry = next;
    },
    addEventListener: events.addEventListener.bind(events),
    removeEventListener: events.removeEventListener.bind(events),
  });
}
function Probe(): ReactElement {
  const owner = useLibraryFilters();
  const [, rerender] = useState(0);
  return (
    <>
      <output>{JSON.stringify({ filters: owner.filters, view: owner.view })}</output>
      <button
        onClick={() => {
          owner.commitAll((current) => ({ ...current, title: { op: "eq", text: "the sea " } }));
          owner.commitAll((current) => ({ ...current, pages: { gte: 300 } }));
        }}
      >
        siblings
      </button>
      <button
        onClick={() => {
          owner.commitAll((current) => ({ ...current, pages: { gte: 999 } }));
          owner.clearAll();
          owner.commitAll((current) => ({ ...current, q: " a " }));
        }}
      >
        blur-clear-type
      </button>
      <button
        onClick={() => {
          owner.setView("table");
          owner.clearAll();
        }}
      >
        view-clear
      </button>
      <button
        onClick={() => {
          rerender((value) => value + 1);
        }}
      >
        render
      </button>
    </>
  );
}
function mount(state?: Record<string, unknown>): void {
  installNavigation(state);
  render(
    <MemoryRouter>
      <Probe />
    </MemoryRouter>,
  );
}
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
test("same-turn sibling edits compose and preserve unrelated native entry data", () => {
  mount({ routerData: { keep: 1 } });
  fireEvent.click(screen.getByText("siblings"));
  expect(entry).toEqual({
    routerData: { keep: 1 },
    reverieLibrary: {
      filters: {
        ...emptyFilterState(),
        title: { op: "eq", text: "the sea " },
        pages: { gte: 300 },
      },
    },
  });
  expect(screen.getByRole("status")).toHaveTextContent("the sea ");
});
test("blur, Clear and subsequent typing use the current intent without resurrection", () => {
  mount();
  fireEvent.click(screen.getByText("siblings"));
  fireEvent.click(screen.getByText("blur-clear-type"));
  expect(entry.reverieLibrary).toEqual({ filters: { ...emptyFilterState(), q: " a " } });
});
test("clear preserves the visit view override", () => {
  mount();
  fireEvent.click(screen.getByText("view-clear"));
  expect(entry.reverieLibrary).toEqual({ view: "table", filters: emptyFilterState() });
});
test.each([
  undefined,
  { filters: { ...emptyFilterState(), pages: { gte: -1 } }, view: "table" },
  { filters: emptyFilterState(), view: "list" },
])("unusable owned payload starts a clean visit without a recovery write: %j", (payload) => {
  mount({ keep: 1, reverieLibrary: payload });
  expect(screen.getByRole("status")).toHaveTextContent(
    JSON.stringify({ filters: emptyFilterState() }),
  );
  expect(entry).toEqual({ keep: 1, reverieLibrary: payload });
  fireEvent.click(screen.getByText("view-clear"));
  expect(entry).toEqual({
    keep: 1,
    reverieLibrary: { filters: emptyFilterState(), view: "table" },
  });
});
test("restores a complete visit before consumers render", () => {
  const visit = {
    filters: { ...emptyFilterState(), q: "restored", addedAfter: "2026-10-25" },
    view: "table",
  };
  mount({ reverieLibrary: visit });
  expect(JSON.parse(screen.getByRole("status").textContent)).toEqual(visit);
});
test("a fresh entry resets a reused owner instead of writing the departed snapshot", () => {
  mount();
  fireEvent.click(screen.getByText("siblings"));
  act(() => {
    Object.defineProperty(navigation.currentEntry, "key", { value: "fresh" });
  });
  entry = { unrelated: "destination" };
  fireEvent.click(screen.getByText("render"));
  expect(screen.getByRole("status")).toHaveTextContent(
    JSON.stringify({ filters: emptyFilterState() }),
  );
  expect(entry).toEqual({ unrelated: "destination" });
});
