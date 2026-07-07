import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vite-plus/test";

import type { SortField, SortLevelParam } from "@/api";

import { SORT_SUMMARY_ID, SortChips } from "./SortChips";

const LABELS: Record<SortField, string> = {
  title: "Title",
  author: "Authors",
  created_at: "Added",
  pages: "Pages",
};

describe("SortChips", () => {
  test("renders no interactive controls for an empty stack", () => {
    render(<SortChips levels={[]} labels={LABELS} onChange={vi.fn()} />);
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  test("the sr-only summary reflects an empty stack", () => {
    render(<SortChips levels={[]} labels={LABELS} onChange={vi.fn()} />);
    const summary = document.getElementById(SORT_SUMMARY_ID);
    expect(summary).not.toBeNull();
    expect(summary).toHaveTextContent("Not sorted");
  });

  test("summary text names every level in priority order with its direction", () => {
    const levels: SortLevelParam[] = [
      { field: "author", desc: false },
      { field: "created_at", desc: true },
    ];
    render(<SortChips levels={levels} labels={LABELS} onChange={vi.fn()} />);
    expect(document.getElementById(SORT_SUMMARY_ID)).toHaveTextContent(
      "Sorted by Authors ascending, then Added descending",
    );
  });

  test("each chip shows its 1-based priority number", () => {
    const levels: SortLevelParam[] = [
      { field: "title", desc: false },
      { field: "author", desc: true },
    ];
    render(<SortChips levels={levels} labels={LABELS} onChange={vi.fn()} />);
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  test("toggling a chip's direction flips only that level", async () => {
    const onChange = vi.fn();
    const levels: SortLevelParam[] = [
      { field: "title", desc: false },
      { field: "pages", desc: false },
    ];
    render(<SortChips levels={levels} labels={LABELS} onChange={onChange} />);
    const user = userEvent.setup();
    // Ascending currently, so clicking flips it to descending.
    await user.click(
      screen.getByRole("button", { name: "Change Title sort direction (currently ascending)" }),
    );
    expect(onChange).toHaveBeenCalledWith([
      { field: "title", desc: true },
      { field: "pages", desc: false },
    ]);
  });

  test("removing a level drops it and renumbers the remaining chips", async () => {
    const onChange = vi.fn();
    const levels: SortLevelParam[] = [
      { field: "title", desc: false },
      { field: "author", desc: false },
      { field: "pages", desc: false },
    ];
    const { rerender } = render(<SortChips levels={levels} labels={LABELS} onChange={onChange} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Remove Title from sort" }));
    const next: readonly SortLevelParam[] = [
      { field: "author", desc: false },
      { field: "pages", desc: false },
    ];
    expect(onChange).toHaveBeenCalledWith(next);

    rerender(<SortChips levels={next} labels={LABELS} onChange={onChange} />);
    expect(screen.queryByText("3")).not.toBeInTheDocument();
    expect(screen.getAllByText("1")).toHaveLength(1);
    expect(screen.getAllByText("2")).toHaveLength(1);
  });

  test("moving a level down swaps it with its neighbor", async () => {
    const onChange = vi.fn();
    const levels: SortLevelParam[] = [
      { field: "title", desc: false },
      { field: "author", desc: false },
    ];
    render(<SortChips levels={levels} labels={LABELS} onChange={onChange} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Move Title later in sort priority" }));
    expect(onChange).toHaveBeenCalledWith([
      { field: "author", desc: false },
      { field: "title", desc: false },
    ]);
  });

  test("move buttons are disabled at the respective ends of the stack", () => {
    const levels: SortLevelParam[] = [
      { field: "title", desc: false },
      { field: "author", desc: false },
    ];
    render(<SortChips levels={levels} labels={LABELS} onChange={vi.fn()} />);
    expect(
      screen.getByRole("button", { name: "Move Title earlier in sort priority" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Move Authors later in sort priority" }),
    ).toBeDisabled();
  });

  test("clear sort empties the stack", async () => {
    const onChange = vi.fn();
    const levels: SortLevelParam[] = [{ field: "title", desc: false }];
    render(<SortChips levels={levels} labels={LABELS} onChange={onChange} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Clear sort" }));
    expect(onChange).toHaveBeenCalledWith([]);
  });
});
