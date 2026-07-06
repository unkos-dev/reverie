/**
 * Keyboard-shortcuts overlay for the library table view.
 *
 * `?` opens the dialog from anywhere on the page (guarded so typing a
 * literal `?` into a field doesn't hijack it); `Esc` or the close button
 * dismiss it. Focus is captured in a layout effect on the open
 * transition, since layout effects flush before Radix's own
 * passive-effect focus trap (`@radix-ui/react-focus-scope` captures
 * `document.activeElement` in a `useEffect`). That ordering lets the
 * invoker (whichever element was focused when `?` fired, or the
 * trigger button on click) be captured before focus ever moves, then
 * restored on close. This mirrors `CommandPalette.tsx`'s
 * invokerRef/`onCloseAutoFocus` pattern, adapted for a dialog whose
 * open state is owned by the parent rather than an internal toggle
 * hook.
 *
 * The shortcut list documents react-data-grid's native keyboard model
 * (no custom key handling is added by this feature) plus the paging
 * caveat: the list endpoint is forward-only keyset with no total
 * count, so Ctrl+End jumps to the last row currently loaded in memory,
 * not the last row in the library.
 *
 * The editing rows document the vendor's own editing keyboard model
 * (Enter/F2/type-to-open, Escape cancel) plus this feature's Ctrl+Z undo.
 * One deviation from the WAI-ARIA APG grid pattern is called out: F2 opens
 * an editor but, unlike the APG's toggle model, does not also close one.
 * react-data-grid at the pinned tag implements F2-open only, so Escape or
 * Enter are the close paths.
 */
import { useEffect, useLayoutEffect, useRef, type ReactElement } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

/** True when the caller is typing into an editable element. */
function inEditableField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable;
}

/**
 * Registers the global `?` keydown binding that opens the shortcuts
 * overlay. Ignored while typing into a field or while a modifier key
 * is held, so it never steals a literal `?` from prose input.
 */
// oxlint-disable-next-line react/only-export-components -- hook shares the module with the dialog + trigger it opens; splitting would scatter one cohesive feature across files.
export function useShortcutsHotkey(setOpen: (open: boolean) => void): void {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent): void {
      if (
        event.key === "?" &&
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        !inEditableField(event.target)
      ) {
        event.preventDefault();
        setOpen(true);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [setOpen]);
}

interface ShortcutRow {
  keys: string[];
  description: string;
}

const SHORTCUT_ROWS: ShortcutRow[] = [
  { keys: ["↑", "↓", "←", "→"], description: "Move focus one cell" },
  { keys: ["Home", "End"], description: "Jump to the first or last cell in the row" },
  { keys: ["Ctrl", "Home"], description: "Jump to the first loaded row" },
  { keys: ["Ctrl", "End"], description: "Jump to the last loaded row" },
  { keys: ["Page Up", "Page Down"], description: "Scroll one viewport of rows" },
  { keys: ["Tab"], description: "Move to the next cell; exits the grid at the last cell" },
  {
    keys: ["Shift", "Tab"],
    description: "Move to the previous cell; exits the grid at the first cell",
  },
  { keys: ["?"], description: "Open this shortcuts overlay" },
  { keys: ["Esc"], description: "Close this overlay" },
  { keys: ["Enter"], description: "Edit the focused cell" },
  {
    keys: ["F2"],
    description: "Edit the focused cell (opens only; Esc or Enter closes)",
  },
  { keys: ["A", "…", "Z"], description: "Type a character to start editing with that value" },
  { keys: ["Esc"], description: "While editing: discard the edit" },
  { keys: ["Enter"], description: "While editing: commit the edit" },
  { keys: ["Ctrl", "Z"], description: "Undo the most recent edit" },
];

type GridShortcutsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

/** `?`-triggered overlay documenting the table view's keyboard model. */
export function GridShortcutsDialog({
  open,
  onOpenChange,
}: GridShortcutsDialogProps): ReactElement {
  const invokerRef = useRef<HTMLElement | null>(null);
  const wasOpenRef = useRef(open);

  // Layout effects flush before any passive effect in the same commit,
  // including Radix FocusScope's `useEffect` that reads
  // `document.activeElement` to trap focus. Capturing here, gated on
  // the false-to-true transition, guarantees the invoker is whatever
  // had focus immediately before open, never a focus target already
  // moved inside the dialog.
  useLayoutEffect(() => {
    if (open && !wasOpenRef.current) {
      invokerRef.current =
        document.activeElement instanceof HTMLElement ? document.activeElement : null;
    }
    wasOpenRef.current = open;
  }, [open]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        aria-labelledby="grid-shortcuts-title"
        onCloseAutoFocus={(event) => {
          if (invokerRef.current !== null && invokerRef.current.isConnected) {
            event.preventDefault();
            invokerRef.current.focus();
          }
          invokerRef.current = null;
        }}
      >
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
    </Dialog>
  );
}

/** Toolbar affordance that opens {@link GridShortcutsDialog}. */
export function GridShortcutsTrigger({
  onOpenChange,
}: {
  onOpenChange: (open: boolean) => void;
}): ReactElement {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Keyboard shortcuts"
      onClick={() => {
        onOpenChange(true);
      }}
    >
      <kbd className="font-mono text-[0.65rem]">?</kbd>
    </Button>
  );
}
