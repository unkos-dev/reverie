/**
 * Autofocused single-line editor for text and positive-integer metadata
 * columns (title, subtitle, isbn_13, pages). Enter/Escape commit or discard
 * at the grid binding's editor container, not here: this component's only
 * job is staging a valid draft via `onDraft` and refusing to stage an
 * invalid one.
 *
 * Invalid-block mechanism: the binding's Enter handler always commits
 * whatever row the last `onDraft` call staged (`update()` in
 * `GridEditorProps`). To block a bad commit without adding an Enter/Escape
 * handler of its own, this editor simply skips the `onDraft` call while the
 * current text fails validation. The staged row stays at its last valid
 * value, so Enter commits that instead of the invalid text still visible
 * in the input. `aria-invalid` surfaces the mismatch in the meantime.
 */
import { useEffect, useRef, useState, type ChangeEvent, type ReactElement } from "react";

import { cn } from "@/lib/utils";

export type TextCellEditorKind = "text" | "positive-int";

type TextCellEditorProps = {
  value: string | null;
  onDraft: (value: string | null) => void;
  kind: TextCellEditorKind;
  required?: boolean;
};

const POSITIVE_INT_PATTERN = /^\d+$/;

function isValidPositiveInt(raw: string): boolean {
  return POSITIVE_INT_PATTERN.test(raw) && Number(raw) > 0;
}

export function TextCellEditor({
  value,
  onDraft,
  kind,
  required = false,
}: TextCellEditorProps): ReactElement {
  const inputRef = useRef<HTMLInputElement>(null);
  const [text, setText] = useState(value ?? "");
  const [invalid, setInvalid] = useState(false);

  // Autofocus + select-on-open mirrors RDG's own built-in text editor
  // (vendored fact: its editor opens pre-focused with content selected).
  // Runs once per mount, which is also once per remount on
  // closeOnExternalRowChange, so it never fights a later prop change.
  useEffect(() => {
    const input = inputRef.current;
    if (input === null) return;
    input.focus();
    input.select();
  }, []);

  function handleChange(event: ChangeEvent<HTMLInputElement>): void {
    const raw = event.target.value;
    setText(raw);

    if (raw === "") {
      setInvalid(required);
      if (!required) onDraft(null);
      return;
    }

    if (kind === "positive-int" && !isValidPositiveInt(raw)) {
      setInvalid(true);
      return;
    }

    setInvalid(false);
    onDraft(raw);
  }

  return (
    <input
      ref={inputRef}
      type="text"
      inputMode={kind === "positive-int" ? "numeric" : undefined}
      value={text}
      onChange={handleChange}
      aria-invalid={invalid || undefined}
      aria-required={required || undefined}
      className={cn(
        "h-8 w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 py-1 text-base outline-none",
        "focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50",
        "aria-invalid:border-danger aria-invalid:ring-3 aria-invalid:ring-danger/20",
      )}
    />
  );
}
