import { render } from "vitest-browser-react";
import { page, userEvent, type Locator } from "vite-plus/test/browser";
import { useState, type ReactElement } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { DateRangeEditor, RangeFilterEditor } from "@/components/library/editors";
import type { RangeFilter } from "@/routes/library-params";

afterEach(() => {
  vi.useRealTimers();
});

function NumericHarness({ rating = false }: { rating?: boolean }): ReactElement {
  const [value, setValue] = useState<RangeFilter>(rating ? {} : { gte: 100, lte: 200 });
  return (
    <>
      <RangeFilterEditor
        label={rating ? "Rating" : "Pages"}
        value={value}
        min={rating ? 1 : 0}
        max={rating ? 5 : 2147483647}
        allowEmpty
        onChange={setValue}
      />
      <output>{JSON.stringify(value)}</output>
    </>
  );
}
async function enter(input: Locator, value: string): Promise<void> {
  await userEvent.clear(input);
  await userEvent.type(input, value);
  await userEvent.keyboard("{Enter}");
}
test.each([true, false])(
  "numeric endpoints cross independently, lower first=%s",
  async (lowerFirst) => {
    await render(<NumericHarness />);
    const lower = page.getByRole("textbox", { name: "Pages minimum" });
    const upper = page.getByRole("textbox", { name: "Pages maximum" });
    if (lowerFirst) {
      await enter(lower, "300");
      await expect.element(upper).toHaveValue("200");
      await expect
        .element(page.getByText("Minimum is above maximum. No books match this range."))
        .toBeVisible();
      await enter(upper, "400");
    } else {
      await enter(upper, "400");
      await enter(lower, "300");
    }
    await expect.element(page.getByRole("status")).toHaveTextContent('{"gte":300,"lte":400}');
    await userEvent.clear(lower);
    await userEvent.tab();
    await expect.element(page.getByRole("status")).toHaveTextContent('{"lte":400}');
  },
);
test("native integer snap, stepping and rating bounds retain optional empty mapping", async () => {
  await render(<NumericHarness rating />);
  const lower = page.getByRole("textbox", { name: "Rating minimum" });
  await enter(lower, "2.7");
  await expect.element(lower).toHaveValue("3");
  await userEvent.click(page.getByRole("button", { name: /^Increase/ }).first());
  await expect.element(lower).toHaveValue("4");
  await enter(lower, "9");
  await expect.element(lower).toHaveValue("5");
  await expect.element(page.getByRole("button", { name: /^Increase/ }).first()).toBeDisabled();
  await enter(lower, "0");
  await expect.element(lower).toHaveValue("1");
  await userEvent.click(page.getByText("Has no value"));
  await expect.element(lower).toBeDisabled();
  await expect.element(page.getByRole("status")).toHaveTextContent('{"empty":true}');
});
test("pages accept their intrinsic maximum with native formatting and independent labels", async () => {
  await render(
    <>
      <NumericHarness />
      <RangeFilterEditor
        label="Other pages"
        value={{}}
        min={0}
        max={2147483647}
        onChange={() => {}}
      />
    </>,
  );
  await enter(page.getByRole("textbox", { name: "Pages minimum" }), "2147483647");
  await expect
    .poll((): unknown => JSON.parse(page.getByRole("status").element().textContent))
    .toEqual({ gte: 2147483647, lte: 200 });
  await expect.element(page.getByRole("textbox", { name: "Other pages minimum" })).toHaveValue("");
  await expect.element(page.getByRole("button", { name: /^Increase/ }).first()).toBeDisabled();
});
function DateHarness(): ReactElement {
  const [value, setValue] = useState<{ after?: string; before?: string }>({ after: "2026-10-25" });
  return (
    <>
      <DateRangeEditor {...value} onChange={setValue} />
      <output>{JSON.stringify(value)}</output>
    </>
  );
}
test("date segments and calendars map optional independent endpoints", async () => {
  vi.setSystemTime(new Date("2026-10-25T04:00:00Z"));
  await render(<DateHarness />);
  const before = page.getByRole("group", { name: "Added on or before" });
  await userEvent.click(before.getByRole("button"));
  const calendar = page.getByRole("grid");
  await userEvent.click(calendar.getByRole("button", { name: /October 20, 2026/ }));
  await expect
    .poll((): unknown => JSON.parse(page.getByRole("status").element().textContent))
    .toEqual({
      after: "2026-10-25",
      before: "2026-10-20",
    });
  await expect
    .element(page.getByText("Start date is after end date. No books match this range."))
    .toBeVisible();
  const year = before.getByRole("spinbutton", { name: /year/i });
  await userEvent.click(year);
  await userEvent.keyboard("2027");
  await expect
    .poll((): unknown => JSON.parse(page.getByRole("status").element().textContent))
    .toEqual({
      after: "2026-10-25",
      before: "2027-10-20",
    });
  await userEvent.click(year);
  await userEvent.keyboard("{Backspace}{Backspace}{Backspace}{Backspace}");
  await userEvent.click(before.getByRole("spinbutton", { name: /month/i }));
  await userEvent.keyboard("{Backspace}{Backspace}");
  await userEvent.click(before.getByRole("spinbutton", { name: /day/i }));
  await userEvent.keyboard("{Backspace}{Backspace}");
  await userEvent.tab();
  await expect.element(page.getByRole("status")).toHaveTextContent('{"after":"2026-10-25"}');
});
test("partial date input survives a peer render without changing its last published endpoint", async () => {
  function PeerHarness({ before }: { before: string }): ReactElement {
    const [dates, setDates] = useState<{ after?: string; before?: string }>({
      after: "2026-10-25",
    });
    return (
      <DateRangeEditor
        after={dates.after}
        before={before}
        onChange={(patch) => {
          setDates((current) => patch({ ...current, before }));
        }}
      />
    );
  }
  const { rerender } = await render(<PeerHarness before="2026-10-25" />);
  const after = page.getByRole("group", { name: "Added on or after" });
  const year = after.getByRole("spinbutton", { name: /year/i });
  await userEvent.click(year);
  await userEvent.keyboard("{Backspace}{Backspace}{Backspace}{Backspace}");
  await expect.element(year).toHaveAttribute("data-placeholder");
  await rerender(<PeerHarness before="2026-11-01" />);
  await expect.element(year).toHaveAttribute("data-placeholder");
  await expect.element(after.getByRole("spinbutton", { name: /day/i })).toHaveTextContent("25");
  await expect
    .element(page.getByText("Start date is after end date. No books match this range."))
    .not.toBeInTheDocument();
});
test("native impossible-date correction remains usable and calendar dismissal returns focus", async () => {
  await render(<DateHarness />);
  const after = page.getByRole("group", { name: "Added on or after" });
  await userEvent.click(after.getByRole("spinbutton", { name: /month/i }));
  await userEvent.keyboard("2");
  await userEvent.click(after.getByRole("spinbutton", { name: /day/i }));
  await userEvent.keyboard("31");
  await userEvent.tab();
  await expect.element(page.getByRole("status")).toHaveTextContent('{"after":"2026-02-28"}');
  const trigger = after.getByRole("button");
  await userEvent.click(trigger);
  await expect.element(page.getByRole("grid")).toBeInTheDocument();
  await userEvent.keyboard("{Escape}");
  await expect.element(trigger).toHaveFocus();
});

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { emptyFilterState, type FilterState, type SetFilter } from "@/routes/library-params";

import {
  StatusEditor,
  TextFilterEditor,
  VocabEditor,
  type TextOp,
  type VocabFamily,
} from "@/components/library/editors";

const TEXT_OPS: readonly TextOp[] = ["contains", "eq", "ne"];

describe("TextFilterEditor", () => {
  test("typing the value reports the active operator", async () => {
    const onChange = vi.fn();
    await render(<TextFilterEditor value={{}} ops={TEXT_OPS} onChange={onChange} />);

    await userEvent.fill(page.getByRole("textbox", { name: "Filter value" }), "dune");

    expect(onChange).toHaveBeenLastCalledWith({ op: "contains", text: "dune" });
  });

  test("only the allowed operators are offered", async () => {
    await render(<TextFilterEditor value={{}} ops={["contains", "empty"]} onChange={vi.fn()} />);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: /operator/i }));

    await expect.element(page.getByRole("option", { name: "contains" })).toBeInTheDocument();
    await expect.element(page.getByRole("option", { name: "is empty" })).toBeInTheDocument();
    await expect.element(page.getByRole("option", { name: "equals" })).not.toBeInTheDocument();
  });

  test("switching to `is empty` reports empty and hides the value input", async () => {
    const onChange = vi.fn();
    function Harness(): ReactElement {
      const [value, setValue] = useState({});
      return (
        <TextFilterEditor
          value={value}
          ops={["contains", "empty"]}
          onChange={(next) => {
            onChange(next);
            setValue(next);
          }}
        />
      );
    }
    await render(<Harness />);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: /operator/i }));
    await user.click(page.getByRole("option", { name: "is empty" }));

    expect(onChange).toHaveBeenLastCalledWith({ op: "empty", text: "" });
    await expect
      .element(page.getByRole("textbox", { name: "Filter value" }))
      .not.toBeInTheDocument();
  });
});

describe("StatusEditor", () => {
  test("checking a status adds its token", async () => {
    const onChange = vi.fn();
    await render(<StatusEditor value={[]} onChange={onChange} />);
    const user = userEvent;

    await user.click(page.getByLabelText("Unread"));

    expect(onChange).toHaveBeenLastCalledWith(["unread"]);
  });

  test("unchecking a status removes its token", async () => {
    const onChange = vi.fn();
    await render(<StatusEditor value={["unread", "reading"]} onChange={onChange} />);
    const user = userEvent;

    await user.click(page.getByLabelText("Unread"));

    expect(onChange).toHaveBeenLastCalledWith(["reading"]);
  });
});

describe("editor id scoping", () => {
  test("two named status groups keep their native labels and changes independent", async () => {
    const firstChange = vi.fn();
    const secondChange = vi.fn();
    await render(
      <>
        <StatusEditor value={[]} onChange={firstChange} />
        <StatusEditor value={["reading"]} onChange={secondChange} />
      </>,
    );
    const user = userEvent;
    const [first, second] = page.getByRole("group", { name: "Reading status" }).all();
    await user.click(first.getByText("Unread"));
    expect(firstChange).toHaveBeenCalledExactlyOnceWith(["unread"]);
    expect(secondChange).not.toHaveBeenCalled();
    const reading = second.getByRole("checkbox", { name: "Reading" });
    reading.element().focus();
    await user.keyboard(" ");
    expect(secondChange).toHaveBeenCalledExactlyOnceWith([]);
    expect(firstChange).toHaveBeenCalledTimes(1);
    await expect.element(reading).toHaveFocus();
  });
});

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function mockSuggest(suggestions: { id: string | null; value: string }[]): void {
  vi.spyOn(globalThis, "fetch").mockImplementation(() =>
    Promise.resolve(jsonResponse({ suggestions })),
  );
}

function VocabHarness({
  initial,
  family = "tags",
}: {
  initial: SetFilter;
  family?: VocabFamily;
}): ReactElement {
  const [client] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: false } } }),
  );
  const [draft, setDraft] = useState<FilterState>({ ...emptyFilterState(), [family]: initial });
  return (
    <QueryClientProvider client={client}>
      <VocabEditor
        family={family}
        draft={draft}
        setDraft={setDraft}
        resolveAuthorLabel={(id) => id}
      />
      <output data-testid="vocab-draft">{JSON.stringify(draft[family])}</output>
    </QueryClientProvider>
  );
}

describe("VocabEditor", () => {
  beforeEach(() => {
    __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
    vi.restoreAllMocks();
  });

  test("mode switch preserves per-mode lists", async () => {
    await render(
      <VocabHarness initial={{ all: ["fiction"], any: ["mystery"], none: ["romance"] }} />,
    );
    const user = userEvent;

    await expect.element(page.getByRole("button", { name: /remove mystery/i })).toBeInTheDocument();

    await user.click(page.getByRole("button", { name: /match mode/i }));
    await user.click(page.getByRole("option", { name: "all of" }));
    await expect.element(page.getByRole("button", { name: /remove fiction/i })).toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: /remove mystery/i }))
      .not.toBeInTheDocument();

    await user.click(page.getByRole("button", { name: /match mode/i }));
    await user.click(page.getByRole("option", { name: "none of" }));
    await expect.element(page.getByRole("button", { name: /remove romance/i })).toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: /remove fiction/i }))
      .not.toBeInTheDocument();
  });

  test("adding a token that exists in another mode removes it from that other mode", async () => {
    mockSuggest([{ id: null, value: "mystery" }]);
    await render(<VocabHarness initial={{ all: [], any: ["mystery"], none: [] }} />);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: /match mode/i }));
    await user.click(page.getByRole("option", { name: "all of" }));

    const input = page.getByRole("combobox", { name: /add tags/i });
    await user.type(input, "my");
    await user.click(page.getByRole("option", { name: "mystery" }));
    await user.keyboard("{Escape}");

    await expect.element(page.getByRole("button", { name: /remove mystery/i })).toBeInTheDocument();

    await user.click(page.getByRole("button", { name: /match mode/i }));
    await user.click(page.getByRole("option", { name: "any of" }));
    await expect
      .element(page.getByRole("button", { name: /remove mystery/i }))
      .not.toBeInTheDocument();
  });

  test("ordinary add and remove round-trips", async () => {
    mockSuggest([{ id: null, value: "fantasy" }]);
    await render(<VocabHarness initial={{ all: [], any: [], none: [] }} />);
    const user = userEvent;

    const input = page.getByRole("combobox", { name: /add tags/i });
    await user.type(input, "fa");
    await user.click(page.getByRole("option", { name: "fantasy" }));

    await user.keyboard("{Escape}");
    const chip = page.getByRole("button", { name: /remove fantasy/i });
    await expect.element(chip).toBeInTheDocument();

    await user.click(chip);
    await expect
      .element(page.getByRole("button", { name: /remove fantasy/i }))
      .not.toBeInTheDocument();
  });
});

test.each(["authors", "tags", "genres", "moods"] as const)(
  "%s commits its persisted identity and keeps mode sets exclusive",
  async (family) => {
    __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
    const id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const value = "Literary";
    const persisted = family === "authors" ? id : value;
    mockSuggest([{ id, value }]);
    await render(
      <VocabHarness family={family} initial={{ all: [], any: [persisted], none: [] }} />,
    );
    const user = userEvent;
    await user.click(page.getByRole("button", { name: /Match mode/ }));
    await user.click(page.getByRole("option", { name: "none of" }));
    await user.type(page.getByRole("combobox"), "li");
    await user.click(page.getByRole("option", { name: value }));
    await user.keyboard("{Escape}");
    expect(JSON.parse(page.getByTestId("vocab-draft").element().textContent)).toEqual({
      all: [],
      any: [],
      none: [persisted],
    });
  },
);
