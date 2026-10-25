import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement } from "react";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import type { SuggestKind } from "@/api";

import { __seedCsrfTokenForTesting } from "@/api/csrf";

import { TypeaheadMultiSelect, type TypeaheadOption } from "./TypeaheadMultiSelect";

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
}: {
  initial?: TypeaheadOption[];
  kind?: SuggestKind;
  disabled?: boolean;
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
        label="author"
        selected={selected}
        onChange={setSelected}
      />
      <output data-testid="selection">{JSON.stringify(selected)}</output>
    </QueryClientProvider>
  );
}

beforeEach(() => {
  // Seed (not reset): apiFetch lazily hydrates an empty cache with a
  // leading /auth/me fetch that would eat these suites' response mocks;
  // hydration behaviour itself is pinned in fetch.test.ts.
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
  vi.restoreAllMocks();
});

describe("TypeaheadMultiSelect", () => {
  test("gates the request below the minimum query length", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() => Promise.resolve(jsonResponse({ suggestions: [] })));
    render(<Harness />);
    const user = userEvent.setup();

    await user.type(screen.getByRole("combobox", { name: /add author/i }), "l");

    expect(await screen.findByText(/at least 2 characters/i)).toBeInTheDocument();
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
    render(<Harness />);
    const user = userEvent.setup();
    const input = screen.getByRole("combobox", { name: /add author/i });
    await user.click(input);
    fireEvent.change(input, { target: { value: "le" } });
    expect(fetchSpy).toHaveBeenCalled();
    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.pathname).toBe("/api/v1/authors/suggest");
    expect(url.searchParams.get("q")).toBe("le");
  });

  test("clicking a suggestion adds a chip and clears the input", async () => {
    mockSuggest([{ id: LE_GUIN, value: "Ursula K. Le Guin" }]);
    render(<Harness />);
    const user = userEvent.setup();
    const input = screen.getByRole("combobox", { name: /add author/i });

    await user.type(input, "le");
    const option = await screen.findByRole("option", { name: "Ursula K. Le Guin" });
    await user.click(option);

    await user.keyboard("{Escape}");
    const chips = screen.getByRole("grid", { name: /selected author/i });
    expect(within(chips).getByRole("button", { name: /Remove Ursula K\. Le Guin/i })).toBeVisible();
    expect(input).toHaveValue("");
  });

  test("a committed option remains selected when the query is retyped", async () => {
    mockSuggest([
      { id: LE_GUIN, value: "Ursula K. Le Guin" },
      { id: LEM, value: "Stanislaw Lem" },
    ]);
    render(<Harness />);
    const user = userEvent.setup();
    const input = screen.getByRole("combobox", { name: /add author/i });

    await user.type(input, "le");
    await user.click(await screen.findByRole("option", { name: "Ursula K. Le Guin" }));

    await user.type(input, "le");
    await waitFor(() => {
      expect(screen.getByRole("option", { name: "Stanislaw Lem" })).toBeInTheDocument();
    });
    expect(screen.getByRole("option", { name: "Ursula K. Le Guin" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await user.click(screen.getByRole("option", { name: "Ursula K. Le Guin" }));
    expect(screen.getByTestId("selection")).toHaveTextContent("[]");
    expect(input).toHaveValue("");
    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("button", { name: /Remove Ursula K\. Le Guin/i }),
    ).not.toBeInTheDocument();
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
      render(<Harness initial={initial} />);
      const user = userEvent.setup();
      const input = screen.getByRole("combobox");
      await user.type(input, "le");
      await screen.findByRole("option", { name: "Octavia Butler" });

      if (action === "add") {
        await user.click(screen.getByRole("option", { name: "Octavia Butler" }));
      } else {
        await user.click(screen.getByTestId("selection"));
      }

      const expected = action === "add" ? [...initial, added] : initial;
      expect(JSON.parse(screen.getByTestId("selection").textContent)).toEqual(expected);
      expect(input).toHaveValue("");
      await user.keyboard("{Escape}");
      const chips = screen.getByRole("grid", { name: /selected author/i });
      expect(
        within(chips)
          .getAllByRole("row")
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
    render(<Harness />);
    const user = userEvent.setup();
    const input = screen.getByRole("combobox");
    await user.type(input, "le");
    await user.click(await screen.findByRole("option", { name: "Ursula K. Le Guin" }));
    await user.type(input, "st");
    await screen.findByRole("option", { name: "Stanislaw Lem" });
    expect(screen.getAllByRole("option").map((option) => option.textContent)).toEqual([
      "Stanislaw Lem",
    ]);
    await user.keyboard("{Escape}");
    expect(screen.getByRole("button", { name: /Remove Ursula K\. Le Guin/i })).toBeVisible();
    await user.type(input, "st");
    await user.click(await screen.findByRole("option", { name: "Stanislaw Lem" }));
    await user.keyboard("{Escape}");
    expect(JSON.parse(screen.getByTestId("selection").textContent)).toEqual([
      { id: LE_GUIN, value: "Ursula K. Le Guin" },
      { id: LEM, value: "Stanislaw Lem" },
    ]);
    const chips = screen.getByRole("grid", { name: /selected author/i });
    expect(
      within(chips)
        .getAllByRole("row")
        .map((row) => row.textContent),
    ).toEqual(["Ursula K. Le Guin", "Stanislaw Lem"]);
  });

  test.each([
    ["l", "Type at least 2 characters to search."],
    ["zz", "No matches."],
  ])("selected chips stay out of suggestions for query %s", async (query, status) => {
    mockSuggest([]);
    render(<Harness initial={[{ id: LE_GUIN, value: "Ursula K. Le Guin" }]} />);
    const user = userEvent.setup();
    const input = screen.getByRole("combobox");
    await user.type(input, query);
    expect(await screen.findByRole("status")).toHaveTextContent(status);
    expect(screen.queryAllByRole("option")).toHaveLength(0);
    expect(input).toHaveAttribute("aria-expanded", "true");
    await user.keyboard("{Escape}");
    const chip = screen.getByRole("button", { name: /Remove Ursula K\. Le Guin/i });
    expect(chip).toBeVisible();
    expect(input).toHaveAccessibleDescription(/Ursula K\. Le Guin/);
    const descriptions = input.getAttribute("aria-describedby")?.split(" ") ?? [];
    expect(descriptions.some((id) => document.getElementById(id)?.contains(chip))).toBe(true);
  });

  test("removes an off-query chip after native popup dismissal", async () => {
    mockSuggest([{ id: LEM, value: "Stanislaw Lem" }]);
    render(<Harness initial={[{ id: LE_GUIN, value: "Ursula K. Le Guin" }]} />);
    const user = userEvent.setup();
    const input = screen.getByRole("combobox");
    await user.type(input, "st");
    await screen.findByRole("option", { name: "Stanislaw Lem" });
    await user.keyboard("{Escape}");
    expect(input).toHaveValue("");
    await user.click(screen.getByRole("button", { name: /Remove Ursula K\. Le Guin/i }));
    expect(screen.getByTestId("selection")).toHaveTextContent("[]");
    expect(input).toHaveValue("");
    await user.type(input, "st");
    expect(await screen.findByRole("option", { name: "Stanislaw Lem" })).toBeVisible();
  });

  test("removes a chip via its dismiss button", async () => {
    render(<Harness initial={[{ id: LE_GUIN, value: "Ursula K. Le Guin" }]} />);
    const user = userEvent.setup();

    const remove = screen.getByRole("button", { name: /Remove Ursula K\. Le Guin/i });
    expect(remove).toHaveAccessibleName("Remove Ursula K. Le Guin");
    await user.click(remove);

    expect(
      screen.queryByRole("button", { name: /Remove Ursula K\. Le Guin/i }),
    ).not.toBeInTheDocument();
  });

  test("Delete removes a focused tag through native keyboard interaction", async () => {
    render(
      <Harness
        initial={[
          { id: LE_GUIN, value: "Ursula K. Le Guin" },
          { id: LEM, value: "Stanislaw Lem" },
        ]}
      />,
    );
    const user = userEvent.setup();
    const tag = screen.getByRole("row", { name: /Stanislaw Lem/ });
    tag.focus();
    await user.keyboard("{Delete}");
    expect(screen.queryByRole("button", { name: /Remove Stanislaw Lem/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Remove Ursula K\. Le Guin/i })).toBeInTheDocument();
  });

  test("commits the highlighted option on Enter and clears the input", async () => {
    mockSuggest([{ id: LE_GUIN, value: "Ursula K. Le Guin" }]);
    render(<Harness />);
    const user = userEvent.setup();
    const input = screen.getByRole("combobox", { name: /add author/i });

    await user.type(input, "le");
    await screen.findByRole("option", { name: "Ursula K. Le Guin" });
    await user.keyboard("{ArrowDown}{Enter}");
    expect(input).toHaveValue("");
    await user.keyboard("{Escape}");

    expect(
      await screen.findByRole("button", { name: /Remove Ursula K\. Le Guin/i }),
    ).toBeInTheDocument();
    expect(input).toHaveValue("");
  });
});

test.each(["tags", "genres", "moods"] as const)(
  "%s matches hydrated labels to suggestion identities",
  async (kind) => {
    mockSuggest([
      { id: LE_GUIN, value: "fiction" },
      { id: LEM, value: "literary" },
    ]);
    render(<Harness kind={kind} initial={[{ value: "fiction" }]} />);
    const user = userEvent.setup();
    await user.type(screen.getByRole("combobox"), "fi");
    expect(await screen.findAllByRole("option", { name: "fiction" })).toHaveLength(1);
    expect(screen.getByRole("option", { name: "fiction" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await user.click(screen.getByRole("option", { name: "literary" }));
    await user.keyboard("{Escape}");
    expect(JSON.parse(screen.getByTestId("selection").textContent)).toEqual([
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
  render(<Harness />);
  const user = userEvent.setup();
  const input = screen.getByRole("combobox");
  await user.type(input, "al");
  await user.click((await screen.findAllByRole("option", { name: "Alex Smith" }))[0]);
  expect(input).toHaveValue("");
  await user.type(input, "alex");
  const options = await screen.findAllByRole("option", { name: "Alex Smith" });
  expect(options[0]).toHaveAttribute("aria-selected", "true");
  await user.click(options[1]);
  expect(input).toHaveValue("");
  await user.keyboard("{Escape}");
  expect(JSON.parse(screen.getByTestId("selection").textContent)).toEqual([
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
  render(<Harness />);
  const user = userEvent.setup();
  const input = screen.getByRole("combobox");
  await user.click(input);
  fireEvent.change(input, { target: { value: "old" } });
  expect(await screen.findByText("Searching…")).toBeInTheDocument();
  fireEvent.change(input, { target: { value: "new" } });
  await user.click(await screen.findByRole("option", { name: "New author" }));
  await act(async () => {
    resolveOld?.(jsonResponse({ suggestions: [{ id: LE_GUIN, value: "Old author" }] }));
    await old;
  });
  expect(input).toHaveValue("");
  expect(screen.queryByRole("option", { name: "Old author" })).not.toBeInTheDocument();
  await user.keyboard("{Escape}");
  expect(JSON.parse(screen.getByTestId("selection").textContent)).toEqual([
    { id: LEM, value: "New author" },
  ]);
});

test("disabled controls cannot change a hydrated selection", async () => {
  render(<Harness disabled initial={[{ id: LE_GUIN, value: "Ursula" }]} />);
  expect(screen.getByRole("combobox")).toBeDisabled();
  expect(screen.getByRole("button", { name: /Show author suggestions/ })).toBeDisabled();
  expect(screen.getByRole("button", { name: /Remove Ursula/ })).toBeDisabled();
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: /Remove Ursula/ }));
  await user.click(screen.getByRole("button", { name: /Show author suggestions/ }));
  expect(JSON.parse(screen.getByTestId("selection").textContent)).toEqual([
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
  render(<Harness />);
  const input = screen.getByRole("combobox");
  await userEvent.setup().click(input);
  fireEvent.change(input, { target: { value: "empty" } });
  expect(await screen.findByText("No matches.")).toBeInTheDocument();
  fireEvent.change(input, { target: { value: "fail" } });
  expect(await screen.findByText("Couldn't load suggestions. Try again.")).toBeInTheDocument();
});
