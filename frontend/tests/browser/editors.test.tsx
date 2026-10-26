import { render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "vite-plus/test/browser";
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
async function enter(input: HTMLElement, value: string): Promise<void> {
  await userEvent.clear(input);
  await userEvent.type(input, value);
  await userEvent.keyboard("{Enter}");
}
test.each([true, false])(
  "numeric endpoints cross independently, lower first=%s",
  async (lowerFirst) => {
    render(<NumericHarness />);
    const lower = screen.getByRole("textbox", { name: "Pages minimum" });
    const upper = screen.getByRole("textbox", { name: "Pages maximum" });
    if (lowerFirst) {
      await enter(lower, "300");
      expect(upper).toHaveValue("200");
      expect(
        screen.getByText("Minimum is above maximum. No books match this range."),
      ).toBeVisible();
      await enter(upper, "400");
    } else {
      await enter(upper, "400");
      await enter(lower, "300");
    }
    expect(screen.getByRole("status")).toHaveTextContent('{"gte":300,"lte":400}');
    await userEvent.clear(lower);
    await userEvent.tab();
    expect(screen.getByRole("status")).toHaveTextContent('{"lte":400}');
  },
);
test("native integer snap, stepping and rating bounds retain optional empty mapping", async () => {
  render(<NumericHarness rating />);
  const lower = screen.getByRole("textbox", { name: "Rating minimum" });
  await enter(lower, "2.7");
  expect(lower).toHaveValue("3");
  await userEvent.click(screen.getAllByRole("button", { name: /^Increase/ })[0]);
  expect(lower).toHaveValue("4");
  await enter(lower, "9");
  expect(lower).toHaveValue("5");
  expect(screen.getAllByRole("button", { name: /^Increase/ })[0]).toBeDisabled();
  await enter(lower, "0");
  expect(lower).toHaveValue("1");
  await userEvent.click(screen.getByText("Has no value"));
  expect(lower).toBeDisabled();
  expect(screen.getByRole("status")).toHaveTextContent('{"empty":true}');
});
test("pages accept their intrinsic maximum with native formatting and independent labels", async () => {
  render(
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
  await enter(screen.getByRole("textbox", { name: "Pages minimum" }), "2147483647");
  expect(JSON.parse(screen.getByRole("status").textContent)).toEqual({ gte: 2147483647, lte: 200 });
  expect(screen.getByRole("textbox", { name: "Other pages minimum" })).toHaveValue("");
  expect(screen.getAllByRole("button", { name: /^Increase/ })[0]).toBeDisabled();
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
  render(<DateHarness />);
  const before = screen.getByRole("group", { name: "Added on or before" });
  await userEvent.click(within(before).getByRole("button"));
  const calendar = await screen.findByRole("grid");
  await userEvent.click(within(calendar).getByRole("button", { name: /October 20, 2026/ }));
  expect(JSON.parse(screen.getByRole("status").textContent)).toEqual({
    after: "2026-10-25",
    before: "2026-10-20",
  });
  expect(
    screen.getByText("Start date is after end date. No books match this range."),
  ).toBeVisible();
  const year = within(before).getByRole("spinbutton", { name: /year/i });
  await userEvent.click(year);
  await userEvent.keyboard("2027");
  expect(JSON.parse(screen.getByRole("status").textContent)).toEqual({
    after: "2026-10-25",
    before: "2027-10-20",
  });
  await userEvent.click(year);
  await userEvent.keyboard("{Backspace}{Backspace}{Backspace}{Backspace}");
  await userEvent.click(within(before).getByRole("spinbutton", { name: /month/i }));
  await userEvent.keyboard("{Backspace}{Backspace}");
  await userEvent.click(within(before).getByRole("spinbutton", { name: /day/i }));
  await userEvent.keyboard("{Backspace}{Backspace}");
  await userEvent.tab();
  expect(screen.getByRole("status")).toHaveTextContent('{"after":"2026-10-25"}');
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
  const { rerender } = render(<PeerHarness before="2026-10-25" />);
  const after = screen.getByRole("group", { name: "Added on or after" });
  const year = within(after).getByRole("spinbutton", { name: /year/i });
  await userEvent.click(year);
  await userEvent.keyboard("{Backspace}{Backspace}{Backspace}{Backspace}");
  expect(year).toHaveAttribute("data-placeholder");
  rerender(<PeerHarness before="2026-11-01" />);
  expect(year).toHaveAttribute("data-placeholder");
  expect(within(after).getByRole("spinbutton", { name: /day/i })).toHaveTextContent("25");
  expect(
    screen.queryByText("Start date is after end date. No books match this range."),
  ).not.toBeInTheDocument();
});
test("native impossible-date correction remains usable and calendar dismissal returns focus", async () => {
  render(<DateHarness />);
  const after = screen.getByRole("group", { name: "Added on or after" });
  await userEvent.click(within(after).getByRole("spinbutton", { name: /month/i }));
  await userEvent.keyboard("2");
  await userEvent.click(within(after).getByRole("spinbutton", { name: /day/i }));
  await userEvent.keyboard("31");
  await userEvent.tab();
  expect(screen.getByRole("status")).toHaveTextContent('{"after":"2026-02-28"}');
  const trigger = within(after).getByRole("button");
  await userEvent.click(trigger);
  await screen.findByRole("grid");
  await userEvent.keyboard("{Escape}");
  await waitFor(() => {
    expect(trigger).toHaveFocus();
  });
});
