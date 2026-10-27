import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { describe, expect, test, vi } from "vite-plus/test";

import { TextCellEditor } from "@/pages/library/table/editors/TextCellEditor";

describe("TextCellEditor", () => {
  test("selects the existing text on mount so typing replaces it, emitting a draft per keystroke", async () => {
    const onDraft = vi.fn();
    const user = userEvent;
    await render(<TextCellEditor value="Dune" onDraft={onDraft} kind="text" />);
    const input = page.getByRole("textbox");
    await expect.element(input).toHaveFocus();

    await user.keyboard("Foundation");

    expect(onDraft).toHaveBeenCalledWith("F");
    expect(onDraft).toHaveBeenLastCalledWith("Foundation");
  });

  test("empty input maps to null for a non-required field", async () => {
    const onDraft = vi.fn();
    const user = userEvent;
    await render(<TextCellEditor value="Dune" onDraft={onDraft} kind="text" />);
    const input = page.getByRole("textbox");

    await user.clear(input);

    expect(onDraft).toHaveBeenLastCalledWith(null);
    await expect.element(input).not.toHaveAttribute("aria-invalid", "true");
  });

  test("a required field blocks an empty draft and surfaces aria-invalid", async () => {
    const onDraft = vi.fn();
    const user = userEvent;
    await render(<TextCellEditor value="Dune" onDraft={onDraft} kind="text" required />);
    const input = page.getByRole("textbox");

    await user.clear(input);

    await expect.element(input).toHaveAttribute("aria-invalid", "true");
    expect(onDraft).not.toHaveBeenCalledWith(null);
  });

  test.each(["abc", "0", "-3"])("positive-int rejects %s and withholds the draft", async (junk) => {
    const onDraft = vi.fn();
    const user = userEvent;
    await render(<TextCellEditor value="100" onDraft={onDraft} kind="positive-int" />);
    const input = page.getByRole("textbox");
    await user.clear(input);
    onDraft.mockClear();

    await user.type(input, junk);

    await expect.element(input).toHaveAttribute("aria-invalid", "true");
    expect(onDraft).not.toHaveBeenCalledWith(junk);
  });

  test("positive-int recovers from an invalid draft: a valid follow-up stages the draft and clears aria-invalid", async () => {
    const onDraft = vi.fn();
    const user = userEvent;
    await render(<TextCellEditor value="100" onDraft={onDraft} kind="positive-int" />);
    const input = page.getByRole("textbox");
    await user.clear(input);
    onDraft.mockClear();

    await user.type(input, "abc");

    await expect.element(input).toHaveAttribute("aria-invalid", "true");
    expect(onDraft).not.toHaveBeenCalledWith("abc");

    await user.clear(input);
    await user.type(input, "412");

    expect(onDraft).toHaveBeenLastCalledWith("412");
    await expect.element(input).not.toHaveAttribute("aria-invalid", "true");
  });

  test("positive-int accepts a valid page count", async () => {
    const onDraft = vi.fn();
    const user = userEvent;
    await render(<TextCellEditor value={null} onDraft={onDraft} kind="positive-int" />);
    const input = page.getByRole("textbox");

    await user.type(input, "412");

    expect(onDraft).toHaveBeenLastCalledWith("412");
    await expect.element(input).not.toHaveAttribute("aria-invalid", "true");
  });
});
