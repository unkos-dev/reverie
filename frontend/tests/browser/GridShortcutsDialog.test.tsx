import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { useState, type ReactElement } from "react";
import { describe, expect, test } from "vite-plus/test";

import {
  GridShortcutsDialog,
  GridShortcutsTrigger,
  useShortcutsHotkey,
} from "@/pages/library/table/GridShortcutsDialog";

function Harness(): ReactElement {
  const [open, setOpen] = useState(false);
  useShortcutsHotkey(setOpen);
  return (
    <div>
      <input aria-label="title filter" />
      <GridShortcutsTrigger onOpenChange={setOpen} />
      <GridShortcutsDialog open={open} onOpenChange={setOpen} />
    </div>
  );
}

async function pressQuestionMark(): Promise<void> {
  await userEvent.keyboard("?");
}

describe("GridShortcutsDialog: '?' hotkey", () => {
  test("is closed by default", async () => {
    await render(<Harness />);
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
  });

  test("opens on '?' from anywhere on the page", async () => {
    await render(<Harness />);
    await pressQuestionMark();
    await expect
      .element(page.getByRole("dialog", { name: /keyboard shortcuts/i }))
      .toBeInTheDocument();
  });

  test("ignores '?' while typing in a field", async () => {
    await render(<Harness />);
    const input = page.getByLabelText("title filter");
    await userEvent.click(input);
    await pressQuestionMark();
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
  });
});

describe("GridShortcutsDialog: trigger button", () => {
  test("opens the dialog on click", async () => {
    await render(<Harness />);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: /keyboard shortcuts/i }));
    await expect
      .element(page.getByRole("dialog", { name: /keyboard shortcuts/i }))
      .toBeInTheDocument();
  });
});

describe("GridShortcutsDialog: close behavior", () => {
  test("Escape closes the dialog", async () => {
    await render(<Harness />);
    const user = userEvent;
    await pressQuestionMark();
    await expect.element(page.getByRole("dialog")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
  });

  test("focus returns to the trigger button after Escape", async () => {
    await render(<Harness />);
    const user = userEvent;
    const trigger = page.getByRole("button", { name: /keyboard shortcuts/i });
    await user.click(trigger);
    await expect.element(page.getByRole("dialog")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    await expect.element(trigger).toHaveFocus();
  });

  test("focus returns to an arbitrary invoker element after the '?' hotkey", async () => {
    await render(<Harness />);
    const user = userEvent;
    const invoker = document.createElement("button");
    invoker.textContent = "some other control";
    document.body.appendChild(invoker);
    invoker.focus();
    await pressQuestionMark();
    await expect.element(page.getByRole("dialog")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    await expect.element(invoker).toHaveFocus();
    invoker.remove();
  });
});

describe("GridShortcutsDialog: content", () => {
  test("lists the react-data-grid keyboard model, including the loaded-row Ctrl+End caveat", async () => {
    await render(<Harness />);
    await pressQuestionMark();
    await expect.element(page.getByRole("dialog")).toBeInTheDocument();
    await expect.element(page.getByText(/last loaded row/i)).toBeInTheDocument();
    expect(page.getByText(/exits the grid/i).elements().length).toBeGreaterThan(0);
  });
});
