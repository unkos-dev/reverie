import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { describe, expect, test, vi } from "vite-plus/test";

import { RatingCellEditor } from "@/pages/library/table/editors/RatingCellEditor";

describe("RatingCellEditor", () => {
  test("exposes a labelled group and a roving aria-label per star", async () => {
    await render(<RatingCellEditor value={3} onCommit={vi.fn()} />);
    await expect.element(page.getByRole("group", { name: "Rating" })).toBeInTheDocument();
    await page.screenshot({ path: "test-results/browser/library-rating.png" });
    for (const n of [1, 2, 3, 4, 5]) {
      await expect
        .element(page.getByRole("button", { name: `Rate ${String(n)} of 5` }))
        .toBeInTheDocument();
    }
  });

  test("digit 3 commits a rating of 3", async () => {
    const onCommit = vi.fn();
    const user = userEvent;
    await render(<RatingCellEditor value={null} onCommit={onCommit} />);
    await expect.element(page.getByRole("group", { name: "Rating" })).toHaveFocus();

    await user.keyboard("3");

    expect(onCommit).toHaveBeenCalledWith(3);
  });

  test("0 commits null", async () => {
    const onCommit = vi.fn();
    const user = userEvent;
    await render(<RatingCellEditor value={4} onCommit={onCommit} />);
    await expect.element(page.getByRole("group", { name: "Rating" })).toHaveFocus();

    await user.keyboard("0");

    expect(onCommit).toHaveBeenCalledWith(null);
  });

  test("Delete commits null", async () => {
    const onCommit = vi.fn();
    const user = userEvent;
    await render(<RatingCellEditor value={4} onCommit={onCommit} />);
    await expect.element(page.getByRole("group", { name: "Rating" })).toHaveFocus();

    await user.keyboard("{Delete}");

    expect(onCommit).toHaveBeenCalledWith(null);
  });

  test("arrows stage a draft without committing, Enter then commits it", async () => {
    const onCommit = vi.fn();
    const onDraft = vi.fn();
    const user = userEvent;
    await render(<RatingCellEditor value={2} onCommit={onCommit} onDraft={onDraft} />);
    await expect.element(page.getByRole("group", { name: "Rating" })).toHaveFocus();

    await user.keyboard("{ArrowRight}");
    expect(onDraft).toHaveBeenLastCalledWith(3);
    expect(onCommit).not.toHaveBeenCalled();

    await user.keyboard("{Enter}");
    expect(onCommit).toHaveBeenCalledWith(3);
  });

  test("clicking a star commits that rating", async () => {
    const onCommit = vi.fn();
    const user = userEvent;
    await render(<RatingCellEditor value={null} onCommit={onCommit} />);

    await user.click(page.getByRole("button", { name: "Rate 5 of 5" }));

    expect(onCommit).toHaveBeenCalledWith(5);
  });

  test("the clear button commits null", async () => {
    const onCommit = vi.fn();
    const user = userEvent;
    await render(<RatingCellEditor value={2} onCommit={onCommit} />);

    await user.click(page.getByRole("button", { name: "Clear rating" }));

    expect(onCommit).toHaveBeenCalledWith(null);
  });
});
