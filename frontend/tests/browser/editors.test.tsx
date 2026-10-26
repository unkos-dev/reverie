import { render } from "vitest-browser-react";
import { page, userEvent, type Locator } from "vite-plus/test/browser";
import { useState, type ReactElement } from "react";
import { afterEach, expect, test, vi } from "vite-plus/test";
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
