import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { describe, expect, test, vi } from "vite-plus/test";

import { AuthorsCellEditor } from "@/pages/library/table/editors/AuthorsCellEditor";

describe("AuthorsCellEditor", () => {
  test("renders one input per existing author", async () => {
    await render(
      <AuthorsCellEditor
        authors={["Frank Herbert", "Brian Herbert"]}
        onCommit={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    await expect.element(page.getByLabelText("Author 1")).toHaveValue("Frank Herbert");
    await expect.element(page.getByLabelText("Author 2")).toHaveValue("Brian Herbert");
    await page.screenshot({ path: "test-results/browser/library-authors.png" });
  });

  test("adding an author and saving commits the extended author list", async () => {
    const onCommit = vi.fn();
    const user = userEvent;
    await render(
      <AuthorsCellEditor authors={["Frank Herbert"]} onCommit={onCommit} onCancel={vi.fn()} />,
    );

    await user.type(page.getByLabelText("Add author"), "Brian Herbert");
    await user.click(page.getByRole("button", { name: "Add" }));
    await user.click(page.getByRole("button", { name: "Save" }));

    expect(onCommit).toHaveBeenCalledWith(["Frank Herbert", "Brian Herbert"]);
  });

  test("removing an author and saving commits the shortened list", async () => {
    const onCommit = vi.fn();
    const user = userEvent;
    await render(
      <AuthorsCellEditor
        authors={["Frank Herbert", "Brian Herbert"]}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />,
    );

    await user.click(page.getByRole("button", { name: "Remove Brian Herbert" }));
    await user.click(page.getByRole("button", { name: "Save" }));

    expect(onCommit).toHaveBeenCalledWith(["Frank Herbert"]);
  });

  test("save is disabled once the last author is removed, with an inline hint", async () => {
    const user = userEvent;
    await render(
      <AuthorsCellEditor authors={["Frank Herbert"]} onCommit={vi.fn()} onCancel={vi.fn()} />,
    );

    await user.click(page.getByRole("button", { name: "Remove Frank Herbert" }));

    await expect.element(page.getByRole("button", { name: "Save" })).toBeDisabled();
    await expect
      .element(page.getByRole("alert"))
      .toHaveTextContent("At least one author is required.");
  });

  test("Escape cancels", async () => {
    const onCancel = vi.fn();
    const user = userEvent;
    await render(
      <AuthorsCellEditor authors={["Frank Herbert"]} onCommit={vi.fn()} onCancel={onCancel} />,
    );

    await user.keyboard("{Escape}");

    expect(onCancel).toHaveBeenCalled();
  });

  test("the Cancel button cancels without committing", async () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const user = userEvent;
    await render(
      <AuthorsCellEditor authors={["Frank Herbert"]} onCommit={onCommit} onCancel={onCancel} />,
    );

    await user.click(page.getByRole("button", { name: "Cancel" }));

    expect(onCancel).toHaveBeenCalled();
    expect(onCommit).not.toHaveBeenCalled();
  });

  test("Enter inside an author input neither cancels nor commits, and the popover stays open", async () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const user = userEvent;
    await render(
      <AuthorsCellEditor
        authors={["Frank Herbert", "Brian Herbert"]}
        onCommit={onCommit}
        onCancel={onCancel}
      />,
    );
    const input = page.getByLabelText("Author 1");
    await userEvent.click(input);

    await user.keyboard("{Enter}");

    expect(onCommit).not.toHaveBeenCalled();
    expect(onCancel).not.toHaveBeenCalled();
    await expect.element(page.getByLabelText("Author 1")).toBeInTheDocument();
    await expect.element(page.getByLabelText("Author 2")).toBeInTheDocument();
  });

  test("blanking an existing author's name disables Save", async () => {
    const user = userEvent;
    await render(
      <AuthorsCellEditor authors={["Frank Herbert"]} onCommit={vi.fn()} onCancel={vi.fn()} />,
    );

    await user.clear(page.getByLabelText("Author 1"));

    await expect.element(page.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  test("saving trims leading and trailing whitespace from each committed author name", async () => {
    const onCommit = vi.fn();
    const user = userEvent;
    await render(
      <AuthorsCellEditor authors={["Frank Herbert"]} onCommit={onCommit} onCancel={vi.fn()} />,
    );
    const input = page.getByLabelText("Author 1");
    await user.clear(input);
    await user.type(input, "  Frank Herbert  ");

    await user.click(page.getByRole("button", { name: "Save" }));

    expect(onCommit).toHaveBeenCalledWith(["Frank Herbert"]);
  });
});
