/**
 * Radix Popover editor for the authors column. The popover opens the moment
 * this component mounts (mounting an editor already signals edit intent),
 * anchored over the cell it replaces. Author edits are staged locally and
 * only reach the caller on an explicit Save; Escape or an outside click
 * dismisses the popover and cancels instead of half-committing whatever the
 * inputs currently hold.
 *
 * The portal stays a React child of this component (the shared `Popover`
 * primitive's default `Portal` placement) so the grid binding's outside-
 * click detector, which walks the React tree rather than the DOM tree,
 * still reads clicks inside the popover as "inside" the editor.
 */
import { Plus, Trash2 } from "lucide-react";
import { useRef, useState, type KeyboardEvent, type ReactElement } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverAnchor, PopoverContent } from "@/components/ui/popover";

type AuthorsCellEditorProps = {
  authors: readonly string[];
  onCommit: (authors: string[]) => void;
  onCancel: () => void;
};

type DraftAuthor = { id: number; value: string };

export function AuthorsCellEditor({
  authors,
  onCommit,
  onCancel,
}: AuthorsCellEditorProps): ReactElement {
  // Numeric ids exist purely so list items keep a stable React key across
  // add/remove/edit — author names are plain strings on the wire (no id of
  // their own) and are not necessarily unique.
  const [names, setNames] = useState<DraftAuthor[]>(() =>
    authors.map((value, index) => ({ id: index, value })),
  );
  const nextId = useRef(authors.length);
  const [draftName, setDraftName] = useState("");

  function updateName(id: number, next: string): void {
    setNames((current) =>
      current.map((entry) => (entry.id === id ? { ...entry, value: next } : entry)),
    );
  }

  function removeName(id: number): void {
    setNames((current) => current.filter((entry) => entry.id !== id));
  }

  function addName(): void {
    const trimmed = draftName.trim();
    if (trimmed === "") return;
    setNames((current) => [...current, { id: nextId.current++, value: trimmed }]);
    setDraftName("");
  }

  function handleAddKeyDown(event: KeyboardEvent<HTMLInputElement>): void {
    if (event.key !== "Enter") return;
    // Enter here means "add this author," not "commit the row" — this
    // editor has no `update()` draft for the grid binding's own Enter
    // handling to commit, so let it stop here rather than bubble into a
    // discard/commit-of-nothing at the editor container.
    event.preventDefault();
    event.stopPropagation();
    addName();
  }

  // Any dismiss that isn't our own Save/Cancel button (Escape, outside
  // click) lands here; both mean "discard."
  function handleOpenChange(open: boolean): void {
    if (!open) onCancel();
  }

  const canSave = names.length > 0;

  return (
    <Popover open onOpenChange={handleOpenChange}>
      <PopoverAnchor asChild>
        <div className="h-full w-full" />
      </PopoverAnchor>
      <PopoverContent align="start" className="w-80">
        <ol className="flex flex-col gap-1.5">
          {names.map((entry, index) => (
            <li key={entry.id} className="flex items-center gap-1.5">
              <Input
                aria-label={`Author ${String(index + 1)}`}
                value={entry.value}
                onChange={(event) => {
                  updateName(entry.id, event.target.value);
                }}
              />
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={`Remove ${entry.value === "" ? `author ${String(index + 1)}` : entry.value}`}
                onClick={() => {
                  removeName(entry.id);
                }}
              >
                <Trash2 aria-hidden="true" />
              </Button>
            </li>
          ))}
        </ol>
        <div className="mt-2 flex items-center gap-1.5">
          <Input
            aria-label="Add author"
            placeholder="Add author…"
            value={draftName}
            onChange={(event) => {
              setDraftName(event.target.value);
            }}
            onKeyDown={handleAddKeyDown}
          />
          <Button type="button" variant="ghost" size="icon-xs" aria-label="Add" onClick={addName}>
            <Plus aria-hidden="true" />
          </Button>
        </div>
        {canSave ? null : (
          <p role="alert" className="text-danger-text mt-2 text-sm">
            At least one author is required.
          </p>
        )}
        <div className="mt-3 flex justify-end gap-1.5">
          <Button type="button" variant="outline" size="sm" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            type="button"
            size="sm"
            disabled={!canSave}
            onClick={() => {
              onCommit(names.map((entry) => entry.value));
            }}
          >
            Save
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
