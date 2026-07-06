import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vite-plus/test";

import { TextCellEditor } from "./TextCellEditor";

describe("TextCellEditor", () => {
  test("selects the existing text on mount so typing replaces it, emitting a draft per keystroke", async () => {
    const onDraft = vi.fn();
    const user = userEvent.setup();
    render(<TextCellEditor value="Dune" onDraft={onDraft} kind="text" />);
    const input = screen.getByRole("textbox");
    expect(input).toHaveFocus();

    // skipClick: the real editor is never clicked into — it mounts already
    // focused with its text selected, and a click here would collapse that
    // selection before typing starts, defeating the very thing under test.
    await user.type(input, "Foundation", { skipClick: true });

    expect(onDraft).toHaveBeenCalledWith("F");
    expect(onDraft).toHaveBeenLastCalledWith("Foundation");
  });

  test("empty input maps to null for a non-required field", async () => {
    const onDraft = vi.fn();
    const user = userEvent.setup();
    render(<TextCellEditor value="Dune" onDraft={onDraft} kind="text" />);
    const input = screen.getByRole("textbox");

    await user.clear(input);

    expect(onDraft).toHaveBeenLastCalledWith(null);
    expect(input).not.toHaveAttribute("aria-invalid", "true");
  });

  test("a required field blocks an empty draft and surfaces aria-invalid", async () => {
    const onDraft = vi.fn();
    const user = userEvent.setup();
    render(<TextCellEditor value="Dune" onDraft={onDraft} kind="text" required />);
    const input = screen.getByRole("textbox");

    await user.clear(input);

    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(onDraft).not.toHaveBeenCalledWith(null);
  });

  test.each(["abc", "0", "-3"])("positive-int rejects %s and withholds the draft", async (junk) => {
    const onDraft = vi.fn();
    const user = userEvent.setup();
    render(<TextCellEditor value="100" onDraft={onDraft} kind="positive-int" />);
    const input = screen.getByRole("textbox");
    await user.clear(input);
    onDraft.mockClear();

    await user.type(input, junk);

    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(onDraft).not.toHaveBeenCalledWith(junk);
  });

  test("positive-int recovers from an invalid draft: a valid follow-up stages the draft and clears aria-invalid", async () => {
    const onDraft = vi.fn();
    const user = userEvent.setup();
    render(<TextCellEditor value="100" onDraft={onDraft} kind="positive-int" />);
    const input = screen.getByRole("textbox");
    await user.clear(input);
    onDraft.mockClear();

    await user.type(input, "abc");

    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(onDraft).not.toHaveBeenCalledWith("abc");

    await user.clear(input);
    await user.type(input, "412");

    expect(onDraft).toHaveBeenLastCalledWith("412");
    expect(input).not.toHaveAttribute("aria-invalid", "true");
  });

  test("positive-int accepts a valid page count", async () => {
    const onDraft = vi.fn();
    const user = userEvent.setup();
    render(<TextCellEditor value={null} onDraft={onDraft} kind="positive-int" />);
    const input = screen.getByRole("textbox");

    await user.type(input, "412");

    expect(onDraft).toHaveBeenLastCalledWith("412");
    expect(input).not.toHaveAttribute("aria-invalid", "true");
  });
});
