/**
 * Keyboard-shortcuts overlay for the library table view.
 *
 * The visible Help control opens the shared native dialog, which owns dismissal and focus restoration.
 * Ctrl+End reaches the last loaded row, not the last book in the library: the list is keyset-paged.
 */
import type { ReactElement, Ref } from "react";

import { Button } from "@/components/ui/button";
import {
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

type ShortcutRow = {
  keys: string[];
  description: string;
};

const SHORTCUT_ROWS: ShortcutRow[] = [
  {
    keys: ["↑", "↓", "←", "→"],
    description: "Move between rows, cells and the controls in a cell",
  },
  {
    keys: ["Home", "End"],
    description: "On a row: the first or last loaded row. In a cell: the first or last cell",
  },
  { keys: ["Ctrl", "Home"], description: "Jump to the first loaded row" },
  { keys: ["Ctrl", "End"], description: "Jump to the last loaded row" },
  { keys: ["Page Up", "Page Down"], description: "Move one screen of rows" },
  { keys: ["Tab"], description: "Leave the table" },
  { keys: ["Space"], description: "On a row: select or deselect it" },
  { keys: ["Ctrl", "A"], description: "Select every loaded book" },
  { keys: ["Enter"], description: "On a row: open its details. On a cell: edit it" },
  { keys: ["F2"], description: "Edit the focused cell, including the title" },
  { keys: ["A", "…", "Z"], description: "Start editing the focused cell with that character" },
  { keys: ["1", "…", "5"], description: "On a rating: set it. 0 clears it" },
  { keys: ["Enter"], description: "While editing: save" },
  { keys: ["Esc"], description: "While editing: discard. Otherwise: clear the selection" },
  { keys: ["Ctrl", "Z"], description: "Undo the most recent saved edit" },
];

type GridShortcutsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

/** Documents the table view's keyboard model. */
export function GridShortcutsDialog({
  open,
  onOpenChange,
}: Readonly<GridShortcutsDialogProps>): ReactElement {
  return (
    <DialogContent isOpen={open} onOpenChange={onOpenChange} aria-labelledby="grid-shortcuts-title">
      <DialogHeader>
        <DialogTitle id="grid-shortcuts-title">Keyboard shortcuts</DialogTitle>
        <DialogDescription className="sr-only">
          Keyboard shortcuts available in the library table view.
        </DialogDescription>
      </DialogHeader>
      <table className="w-full border-collapse text-sm">
        <caption className="sr-only">Keyboard shortcuts for the library table view</caption>
        <thead>
          <tr className="border-b text-left text-xs text-muted-foreground">
            <th scope="col" className="w-32 py-1.5 pr-3 font-medium">
              Shortcut
            </th>
            <th scope="col" className="py-1.5 font-medium">
              Action
            </th>
          </tr>
        </thead>
        <tbody>
          {SHORTCUT_ROWS.map((row) => (
            <tr key={row.description} className="border-b last:border-0">
              <td className="py-1.5 pr-3 align-top">
                <span className="flex flex-wrap items-center gap-1 font-mono text-[0.65rem]">
                  {row.keys.map((key, index) => (
                    <span key={key} className="flex items-center gap-1">
                      {index > 0 ? <span className="text-muted-foreground">+</span> : null}
                      <kbd>{key}</kbd>
                    </span>
                  ))}
                </span>
              </td>
              <td className="py-1.5 align-top text-foreground">{row.description}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </DialogContent>
  );
}

/** Toolbar affordance that opens {@link GridShortcutsDialog}. */
export function GridShortcutsTrigger({
  onOpenChange,
  buttonRef,
}: Readonly<{
  onOpenChange: (open: boolean) => void;
  buttonRef?: Ref<HTMLButtonElement>;
}>): ReactElement {
  return (
    <Button
      ref={buttonRef}
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Keyboard shortcuts"
      onPress={() => {
        onOpenChange(true);
      }}
    >
      <kbd className="font-mono text-[0.65rem]">?</kbd>
    </Button>
  );
}
