import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement } from "react";
import { describe, expect, test } from "vite-plus/test";

import {
  GridShortcutsDialog,
  GridShortcutsTrigger,
  useShortcutsHotkey,
} from "./GridShortcutsDialog";

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

function pressQuestionMark(target: Window | HTMLElement): void {
  act(() => {
    fireEvent.keyDown(target, { key: "?" });
  });
}

describe("GridShortcutsDialog: '?' hotkey", () => {
  test("is closed by default", () => {
    render(<Harness />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("opens on '?' from anywhere on the page", async () => {
    render(<Harness />);
    pressQuestionMark(window);
    expect(await screen.findByRole("dialog", { name: /keyboard shortcuts/i })).toBeInTheDocument();
  });

  test("ignores '?' while typing in a field", () => {
    render(<Harness />);
    const input = screen.getByLabelText("title filter");
    input.focus();
    pressQuestionMark(input);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

describe("GridShortcutsDialog: trigger button", () => {
  test("opens the dialog on click", async () => {
    render(<Harness />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /keyboard shortcuts/i }));
    expect(await screen.findByRole("dialog", { name: /keyboard shortcuts/i })).toBeInTheDocument();
  });
});

describe("GridShortcutsDialog: close behavior", () => {
  test("Escape closes the dialog", async () => {
    render(<Harness />);
    const user = userEvent.setup();
    pressQuestionMark(window);
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });

  test("focus returns to the trigger button after Escape", async () => {
    render(<Harness />);
    const user = userEvent.setup();
    const trigger = screen.getByRole("button", { name: /keyboard shortcuts/i });
    await user.click(trigger);
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
    await waitFor(() => {
      expect(trigger).toHaveFocus();
    });
  });

  test("focus returns to an arbitrary invoker element after the '?' hotkey", async () => {
    render(<Harness />);
    const user = userEvent.setup();
    const invoker = document.createElement("button");
    invoker.textContent = "some other control";
    document.body.appendChild(invoker);
    invoker.focus();
    pressQuestionMark(window);
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
    await waitFor(() => {
      expect(invoker).toHaveFocus();
    });
    invoker.remove();
  });
});

describe("GridShortcutsDialog: content", () => {
  test("lists the react-data-grid keyboard model, including the loaded-row Ctrl+End caveat", async () => {
    render(<Harness />);
    pressQuestionMark(window);
    await screen.findByRole("dialog");
    expect(screen.getByText(/last loaded row/i)).toBeInTheDocument();
    expect(screen.getAllByText(/exits the grid/i).length).toBeGreaterThan(0);
  });
});
