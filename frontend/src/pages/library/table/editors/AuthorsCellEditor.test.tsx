import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vite-plus/test";

import { AuthorsCellEditor } from "./AuthorsCellEditor";

describe("AuthorsCellEditor", () => {
  test("renders one input per existing author", () => {
    render(
      <AuthorsCellEditor
        authors={["Frank Herbert", "Brian Herbert"]}
        onCommit={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByLabelText("Author 1")).toHaveValue("Frank Herbert");
    expect(screen.getByLabelText("Author 2")).toHaveValue("Brian Herbert");
  });

  test("adding an author and saving commits the extended author list", async () => {
    const onCommit = vi.fn();
    const user = userEvent.setup();
    render(
      <AuthorsCellEditor authors={["Frank Herbert"]} onCommit={onCommit} onCancel={vi.fn()} />,
    );

    await user.type(screen.getByLabelText("Add author"), "Brian Herbert");
    await user.click(screen.getByRole("button", { name: "Add" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onCommit).toHaveBeenCalledWith(["Frank Herbert", "Brian Herbert"]);
  });

  test("removing an author and saving commits the shortened list", async () => {
    const onCommit = vi.fn();
    const user = userEvent.setup();
    render(
      <AuthorsCellEditor
        authors={["Frank Herbert", "Brian Herbert"]}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Remove Brian Herbert" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onCommit).toHaveBeenCalledWith(["Frank Herbert"]);
  });

  test("save is disabled once the last author is removed, with an inline hint", async () => {
    const user = userEvent.setup();
    render(<AuthorsCellEditor authors={["Frank Herbert"]} onCommit={vi.fn()} onCancel={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: "Remove Frank Herbert" }));

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    expect(screen.getByRole("alert")).toHaveTextContent("At least one author is required.");
  });

  test("Escape cancels", async () => {
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(
      <AuthorsCellEditor authors={["Frank Herbert"]} onCommit={vi.fn()} onCancel={onCancel} />,
    );

    await user.keyboard("{Escape}");

    expect(onCancel).toHaveBeenCalled();
  });

  test("the Cancel button cancels without committing", async () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(
      <AuthorsCellEditor authors={["Frank Herbert"]} onCommit={onCommit} onCancel={onCancel} />,
    );

    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onCancel).toHaveBeenCalled();
    expect(onCommit).not.toHaveBeenCalled();
  });

  test("Enter inside an author input neither cancels nor commits, and the popover stays open", async () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(
      <AuthorsCellEditor
        authors={["Frank Herbert", "Brian Herbert"]}
        onCommit={onCommit}
        onCancel={onCancel}
      />,
    );
    const input = screen.getByLabelText("Author 1");
    input.focus();

    await user.keyboard("{Enter}");

    expect(onCommit).not.toHaveBeenCalled();
    expect(onCancel).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Author 1")).toBeInTheDocument();
    expect(screen.getByLabelText("Author 2")).toBeInTheDocument();
  });

  test("blanking an existing author's name disables Save", async () => {
    const user = userEvent.setup();
    render(<AuthorsCellEditor authors={["Frank Herbert"]} onCommit={vi.fn()} onCancel={vi.fn()} />);

    await user.clear(screen.getByLabelText("Author 1"));

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  test("saving trims leading and trailing whitespace from each committed author name", async () => {
    const onCommit = vi.fn();
    const user = userEvent.setup();
    render(
      <AuthorsCellEditor authors={["Frank Herbert"]} onCommit={onCommit} onCancel={vi.fn()} />,
    );
    const input = screen.getByLabelText("Author 1");
    await user.clear(input);
    await user.type(input, "  Frank Herbert  ");

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onCommit).toHaveBeenCalledWith(["Frank Herbert"]);
  });
});
