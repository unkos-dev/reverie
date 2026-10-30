import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { MemoryRouter } from "react-router";
import type { ReactElement } from "react";
import { afterEach, expect, test } from "vite-plus/test";
import { emptyFilterState } from "@/routes/library-params";
import { useLibraryFilters } from "@/lib/hooks/use-library-filters";

const initialState: unknown = navigation.currentEntry?.getState();
afterEach(() => {
  navigation.updateCurrentEntry({ state: initialState ?? null });
});

function Probe(): ReactElement {
  const owner = useLibraryFilters();
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
          owner.setView("table");
          owner.clearAll();
        }}
      >
        view-clear
      </button>
    </>
  );
}
async function mount(state: Record<string, unknown> = {}): Promise<void> {
  navigation.updateCurrentEntry({ state });
  await render(
    <MemoryRouter>
      <Probe />
    </MemoryRouter>,
  );
}
function entry(): unknown {
  return navigation.currentEntry?.getState();
}
test("same-turn sibling edits compose and preserve unrelated native entry data", async () => {
  await mount({ routerData: { keep: 1 } });
  await userEvent.click(page.getByText("siblings"));
  expect(entry()).toEqual({
    routerData: { keep: 1 },
    reverieLibrary: {
      filters: {
        ...emptyFilterState(),
        title: { op: "eq", text: "the sea " },
        pages: { gte: 300 },
      },
    },
  });
  await expect.element(page.getByRole("status")).toMatchTextContent("the sea ");
});
test.each([
  undefined,
  { filters: { ...emptyFilterState(), pages: { gte: -1 } }, view: "table" },
  { filters: emptyFilterState(), view: "list" },
])("unusable owned payload starts a clean visit without a recovery write: %j", async (payload) => {
  await mount({ keep: 1, reverieLibrary: payload });
  await expect
    .element(page.getByRole("status"))
    .toMatchTextContent(JSON.stringify({ filters: emptyFilterState() }));
  expect(entry()).toEqual({ keep: 1, reverieLibrary: payload });
  await userEvent.click(page.getByText("view-clear"));
  expect(entry()).toEqual({
    keep: 1,
    reverieLibrary: { filters: emptyFilterState(), view: "table" },
  });
});
