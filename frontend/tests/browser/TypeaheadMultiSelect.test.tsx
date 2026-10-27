import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { act } from "react";
import { useState, type ReactElement } from "react";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import type { SuggestKind } from "@/api";

import { __seedCsrfTokenForTesting } from "@/api/csrf";

import {
  TypeaheadMultiSelect,
  type TypeaheadOption,
} from "@/components/library/TypeaheadMultiSelect";

const LE_GUIN = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const LEM = "bbbbbbbb-bbbb-4bbb-9bbb-bbbbbbbbbbbb";
const BUTLER = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function mockSuggest(suggestions: { id: string; value: string }[]): void {
  vi.spyOn(globalThis, "fetch").mockImplementation(() =>
    Promise.resolve(jsonResponse({ suggestions })),
  );
}

function Harness({
  initial = [],
  kind = "authors",
  disabled = false,
  maxSelectedItems,
}: {
  initial?: TypeaheadOption[];
  kind?: SuggestKind;
  disabled?: boolean;
  maxSelectedItems?: number;
}): ReactElement {
  const [client] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: false } } }),
  );
  const [selected, setSelected] = useState<TypeaheadOption[]>(initial);
  return (
    <QueryClientProvider client={client}>
      <TypeaheadMultiSelect
        kind={kind}
        disabled={disabled}
        maxSelectedItems={maxSelectedItems}
        label="author"
        selected={selected}
        onChange={setSelected}
      />
      <output data-testid="selection">{JSON.stringify(selected)}</output>
    </QueryClientProvider>
  );
}

beforeEach(() => {
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
  vi.restoreAllMocks();
});

describe("TypeaheadMultiSelect", () => {
  test("a caller without a selection limit can add beyond twenty", async () => {
    const initial = Array.from({ length: 20 }, (_, index) => ({ value: `Tag ${String(index)}` }));
    mockSuggest([{ id: LE_GUIN, value: "Another tag" }]);
    await render(<Harness kind="tags" initial={initial} />);
    const user = userEvent;
    await user.type(page.getByRole("combobox"), "Another");
    await user.click(page.getByRole("option", { name: "Another tag" }));
    expect(JSON.parse(page.getByTestId("selection").element().textContent)).toHaveLength(21);
  });
  test("limited author suggestions use UUID identity and keep selected options removable", async () => {
    mockSuggest([
      { id: LE_GUIN, value: "Changed author label" },
      { id: LEM, value: "Another author" },
    ]);
    await render(
      <Harness initial={[{ id: LE_GUIN, value: "Original author label" }]} maxSelectedItems={1} />,
    );
    const user = userEvent;
    await user.type(page.getByRole("combobox"), "author");
    const another = page.getByRole("option", { name: "Another author" });
    await expect.element(another).toHaveAttribute("aria-disabled", "true");
    const selected = page.getByRole("option", { name: "Changed author label" });
    await expect.element(selected).not.toHaveAttribute("aria-disabled", "true");
    await user.click(selected);
    await expect.element(page.getByTestId("selection")).toHaveTextContent("[]");
    await user.type(page.getByRole("combobox"), "author");
    await user.click(page.getByRole("option", { name: "Another author" }));
    expect(JSON.parse(page.getByTestId("selection").element().textContent)).toEqual([
      { id: LEM, value: "Another author" },
    ]);
  });
  test("gates the request below the minimum query length", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() => Promise.resolve(jsonResponse({ suggestions: [] })));
    await render(<Harness />);
    const user = userEvent;

    await user.type(page.getByRole("combobox", { name: /add author/i }), "l");

    await expect.element(page.getByText(/at least 2 characters/i)).toBeInTheDocument();
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  test("starts an eligible suggestion request without an added query delay", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() =>
        Promise.resolve(
          jsonResponse({ suggestions: [{ id: LE_GUIN, value: "Ursula K. Le Guin" }] }),
        ),
      );
    await render(<Harness />);
    const user = userEvent;
    const input = page.getByRole("combobox", { name: /add author/i });
    await user.click(input);
    await userEvent.fill(input, "le");
    expect(fetchSpy).toHaveBeenCalled();
    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.pathname).toBe("/api/v1/authors/suggest");
    expect(url.searchParams.get("q")).toBe("le");
  });

  test("clicking a suggestion adds a chip and clears the input", async () => {
    mockSuggest([{ id: LE_GUIN, value: "Ursula K. Le Guin" }]);
    await render(<Harness />);
    const user = userEvent;
    const input = page.getByRole("combobox", { name: /add author/i });

    await user.type(input, "le");
    const option = page.getByRole("option", { name: "Ursula K. Le Guin" });
    await user.click(option);

    await user.keyboard("{Escape}");
    const chips = page.getByRole("grid", { name: /selected author/i });
    await expect
      .element(chips.getByRole("button", { name: /Remove Ursula K\. Le Guin/i }))
      .toBeVisible();
    await expect.element(input).toHaveValue("");
  });

  test("a committed option remains selected when the query is retyped", async () => {
    mockSuggest([
      { id: LE_GUIN, value: "Ursula K. Le Guin" },
      { id: LEM, value: "Stanislaw Lem" },
    ]);
    await render(<Harness />);
    const user = userEvent;
    const input = page.getByRole("combobox", { name: /add author/i });

    await user.type(input, "le");
    await user.click(page.getByRole("option", { name: "Ursula K. Le Guin" }));

    await user.type(input, "le");
    await expect.element(page.getByRole("option", { name: "Stanislaw Lem" })).toBeInTheDocument();
    await expect
      .element(page.getByRole("option", { name: "Ursula K. Le Guin" }))
      .toHaveAttribute("aria-selected", "true");
    await user.click(page.getByRole("option", { name: "Ursula K. Le Guin" }));
    await expect.element(page.getByTestId("selection")).toHaveTextContent("[]");
    await expect.element(input).toHaveValue("");
    await user.keyboard("{Escape}");
    await expect
      .element(page.getByRole("button", { name: /Remove Ursula K\. Le Guin/i }))
      .not.toBeInTheDocument();
  });

  test.each(["add", "blur"] as const)(
    "%s preserves selection order despite reversed server ranking",
    async (action) => {
      const initial = [
        { id: LE_GUIN, value: "Ursula K. Le Guin" },
        { id: LEM, value: "Stanislaw Lem" },
      ];
      const added = { id: BUTLER, value: "Octavia Butler" };
      mockSuggest([initial[1], { id: LE_GUIN, value: "Ursula Le Guin" }, added]);
      await render(<Harness initial={initial} />);
      const user = userEvent;
      const input = page.getByRole("combobox");
      await user.type(input, "le");
      await expect
        .element(page.getByRole("option", { name: "Octavia Butler" }))
        .toBeInTheDocument();

      if (action === "add") {
        await user.click(page.getByRole("option", { name: "Octavia Butler" }));
      } else {
        await user.click(page.getByTestId("selection"));
      }

      const expected = action === "add" ? [...initial, added] : initial;
      expect(JSON.parse(page.getByTestId("selection").element().textContent)).toEqual(expected);
      await expect.element(input).toHaveValue("");
      await user.keyboard("{Escape}");
      const chips = page.getByRole("grid", { name: /selected author/i });
      expect(
        page
          .elementLocator(chips.element())
          .getByRole("row")
          .elements()
          .map((row) => row.textContent),
      ).toEqual(expected.map((item) => item.value));
    },
  );

  test("disjoint search results retain selected chips without adding them to suggestions", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
      let raw: string;
      if (typeof input === "string") {
        raw = input;
      } else if (input instanceof URL) {
        raw = input.href;
      } else {
        raw = input.url;
      }
      const url = new URL(raw);
      const suggestions =
        url.searchParams.get("q") === "le"
          ? [{ id: LE_GUIN, value: "Ursula K. Le Guin" }]
          : [{ id: LEM, value: "Stanislaw Lem" }];
      return Promise.resolve(jsonResponse({ suggestions }));
    });
    await render(<Harness />);
    const user = userEvent;
    const input = page.getByRole("combobox");
    await user.type(input, "le");
    await user.click(page.getByRole("option", { name: "Ursula K. Le Guin" }));
    await user.type(input, "st");
    await expect.element(page.getByRole("option", { name: "Stanislaw Lem" })).toBeInTheDocument();
    expect(
      page
        .getByRole("option")
        .all()
        .map((option) => option.element().textContent),
    ).toEqual(["Stanislaw Lem"]);
    await user.keyboard("{Escape}");
    await expect
      .element(page.getByRole("button", { name: /Remove Ursula K\. Le Guin/i }))
      .toBeVisible();
    await user.type(input, "st");
    await user.click(page.getByRole("option", { name: "Stanislaw Lem" }));
    await user.keyboard("{Escape}");
    expect(JSON.parse(page.getByTestId("selection").element().textContent)).toEqual([
      { id: LE_GUIN, value: "Ursula K. Le Guin" },
      { id: LEM, value: "Stanislaw Lem" },
    ]);
    const chips = page.getByRole("grid", { name: /selected author/i });
    expect(
      page
        .elementLocator(chips.element())
        .getByRole("row")
        .elements()
        .map((row) => row.textContent),
    ).toEqual(["Ursula K. Le Guin", "Stanislaw Lem"]);
  });

  test.each([
    ["l", "Type at least 2 characters to search."],
    ["zz", "No matches."],
  ])("selected chips stay out of suggestions for query %s", async (query, status) => {
    mockSuggest([]);
    await render(<Harness initial={[{ id: LE_GUIN, value: "Ursula K. Le Guin" }]} />);
    const user = userEvent;
    const input = page.getByRole("combobox");
    await user.type(input, query);
    await expect.element(page.getByRole("status")).toHaveTextContent(status);
    expect(page.getByRole("option").elements()).toHaveLength(0);
    await expect.element(input).toHaveAttribute("aria-expanded", "true");
    await user.keyboard("{Escape}");
    const chip = page.getByRole("button", { name: /Remove Ursula K\. Le Guin/i });
    await expect.element(chip).toBeVisible();
    await expect.element(input).toHaveAccessibleDescription(/Ursula K\. Le Guin/);
    const descriptions = input.element().getAttribute("aria-describedby")?.split(" ") ?? [];
    expect(descriptions.some((id) => document.getElementById(id)?.contains(chip.element()))).toBe(
      true,
    );
  });

  test("removes an off-query chip after native popup dismissal", async () => {
    mockSuggest([{ id: LEM, value: "Stanislaw Lem" }]);
    await render(<Harness initial={[{ id: LE_GUIN, value: "Ursula K. Le Guin" }]} />);
    const user = userEvent;
    const input = page.getByRole("combobox");
    await user.type(input, "st");
    await expect.element(page.getByRole("option", { name: "Stanislaw Lem" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await expect.element(input).toHaveValue("");
    await user.click(page.getByRole("button", { name: /Remove Ursula K\. Le Guin/i }));
    await expect.element(page.getByTestId("selection")).toHaveTextContent("[]");
    await expect.element(input).toHaveValue("");
    await user.type(input, "st");
    await expect.element(page.getByRole("option", { name: "Stanislaw Lem" })).toBeVisible();
  });

  test("removes a chip via its dismiss button", async () => {
    await render(<Harness initial={[{ id: LE_GUIN, value: "Ursula K. Le Guin" }]} />);
    const user = userEvent;

    const remove = page.getByRole("button", { name: /Remove Ursula K\. Le Guin/i });
    expect(remove).toHaveAccessibleName("Remove Ursula K. Le Guin");
    await user.click(remove);

    await expect
      .element(page.getByRole("button", { name: /Remove Ursula K\. Le Guin/i }))
      .not.toBeInTheDocument();
  });

  test("Delete removes a focused tag through native keyboard interaction", async () => {
    await render(
      <Harness
        initial={[
          { id: LE_GUIN, value: "Ursula K. Le Guin" },
          { id: LEM, value: "Stanislaw Lem" },
        ]}
      />,
    );
    const user = userEvent;
    const tag = page.getByRole("row", { name: /Stanislaw Lem/ });
    tag.element().focus();
    await user.keyboard("{Delete}");
    await expect
      .element(page.getByRole("button", { name: /Remove Stanislaw Lem/i }))
      .not.toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: /Remove Ursula K\. Le Guin/i }))
      .toBeInTheDocument();
  });

  test("commits the highlighted option on Enter and clears the input", async () => {
    mockSuggest([{ id: LE_GUIN, value: "Ursula K. Le Guin" }]);
    await render(<Harness />);
    const user = userEvent;
    const input = page.getByRole("combobox", { name: /add author/i });

    await user.type(input, "le");
    await expect
      .element(page.getByRole("option", { name: "Ursula K. Le Guin" }))
      .toBeInTheDocument();
    await user.keyboard("{ArrowDown}{Enter}");
    await expect.element(input).toHaveValue("");
    await user.keyboard("{Escape}");

    await expect
      .element(page.getByRole("button", { name: /Remove Ursula K\. Le Guin/i }))
      .toBeInTheDocument();
    await expect.element(input).toHaveValue("");
  });
});

test.each(["tags", "genres", "moods"] as const)(
  "%s matches hydrated labels to suggestion identities",
  async (kind) => {
    mockSuggest([
      { id: LE_GUIN, value: "fiction" },
      { id: LEM, value: "literary" },
    ]);
    await render(<Harness kind={kind} initial={[{ value: "fiction" }]} />);
    const user = userEvent;
    await user.type(page.getByRole("combobox"), "fi");
    expect(page.getByRole("option", { name: "fiction" }).all()).toHaveLength(1);
    await expect
      .element(page.getByRole("option", { name: "fiction" }))
      .toHaveAttribute("aria-selected", "true");
    await user.click(page.getByRole("option", { name: "literary" }));
    await user.keyboard("{Escape}");
    expect(JSON.parse(page.getByTestId("selection").element().textContent)).toEqual([
      { value: "fiction" },
      { id: LEM, value: "literary" },
    ]);
  },
);

test("same-named authors remain distinct and a second selection clears the new query", async () => {
  mockSuggest([
    { id: LE_GUIN, value: "Alex Smith" },
    { id: LEM, value: "Alex Smith" },
  ]);
  await render(<Harness />);
  const user = userEvent;
  const input = page.getByRole("combobox");
  await user.type(input, "al");
  await user.click(page.getByRole("option", { name: "Alex Smith" }).all()[0]);
  await expect.element(input).toHaveValue("");
  await user.type(input, "alex");
  const options = page.getByRole("option", { name: "Alex Smith" }).all();
  await expect.element(options[0]).toHaveAttribute("aria-selected", "true");
  await user.click(options[1]);
  await expect.element(input).toHaveValue("");
  await user.keyboard("{Escape}");
  expect(JSON.parse(page.getByTestId("selection").element().textContent)).toEqual([
    { id: LE_GUIN, value: "Alex Smith" },
    { id: LEM, value: "Alex Smith" },
  ]);
});

test("late suggestions cannot replace the current query or resurrect committed input", async () => {
  let resolveOld: ((response: Response) => void) | undefined;
  const old = new Promise<Response>((resolve) => {
    resolveOld = resolve;
  });
  vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    let raw: string;
    if (typeof input === "string") {
      raw = input;
    } else if (input instanceof URL) {
      raw = input.href;
    } else {
      raw = input.url;
    }
    const url = new URL(raw);
    return url.searchParams.get("q") === "old"
      ? old
      : Promise.resolve(jsonResponse({ suggestions: [{ id: LEM, value: "New author" }] }));
  });
  await render(<Harness />);
  const user = userEvent;
  const input = page.getByRole("combobox");
  await user.click(input);
  await userEvent.fill(input, "old");
  await expect.element(page.getByText("Searching…")).toBeInTheDocument();
  await userEvent.fill(input, "new");
  await user.click(page.getByRole("option", { name: "New author" }));
  await act(async () => {
    resolveOld?.(jsonResponse({ suggestions: [{ id: LE_GUIN, value: "Old author" }] }));
    await old;
  });
  await expect.element(input).toHaveValue("");
  await expect.element(page.getByRole("option", { name: "Old author" })).not.toBeInTheDocument();
  await user.keyboard("{Escape}");
  expect(JSON.parse(page.getByTestId("selection").element().textContent)).toEqual([
    { id: LEM, value: "New author" },
  ]);
});

test("disabled controls cannot change a hydrated selection", async () => {
  await render(<Harness disabled initial={[{ id: LE_GUIN, value: "Ursula" }]} />);
  await expect.element(page.getByRole("combobox")).toBeDisabled();
  await expect
    .element(page.getByRole("button", { name: /Show author suggestions/ }))
    .toBeDisabled();
  await expect.element(page.getByRole("button", { name: /Remove Ursula/ })).toBeDisabled();
  const user = userEvent;
  await user.click(page.getByRole("button", { name: /Remove Ursula/ }));
  await user.click(page.getByRole("button", { name: /Show author suggestions/ }));
  expect(JSON.parse(page.getByTestId("selection").element().textContent)).toEqual([
    { id: LE_GUIN, value: "Ursula" },
  ]);
});

test("empty and failed suggestion requests expose their status", async () => {
  vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    let raw: string;
    if (typeof input === "string") {
      raw = input;
    } else if (input instanceof URL) {
      raw = input.href;
    } else {
      raw = input.url;
    }
    return new URL(raw).searchParams.get("q") === "fail"
      ? Promise.resolve(new Response("failed", { status: 500 }))
      : Promise.resolve(jsonResponse({ suggestions: [] }));
  });
  await render(<Harness />);
  const input = page.getByRole("combobox");
  await userEvent.click(input);
  await userEvent.fill(input, "empty");
  await expect.element(page.getByText("No matches.")).toBeInTheDocument();
  await userEvent.fill(input, "fail");
  await expect.element(page.getByText("Couldn't load suggestions. Try again.")).toBeInTheDocument();
});
