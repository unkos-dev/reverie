import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { useState, type ReactElement } from "react";
import { expect, test } from "vite-plus/test";
import { DateRangeEditor } from "@/components/library/editors";

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
