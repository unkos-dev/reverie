/**
 * Debounced quick-search input over the library's `q` filter. Lives in
 * the toolbar; participates in the same draft-survival protocol as the
 * filter rail's typed sections through the shared `lastEdit` ref (see
 * `filter-commit.ts`).
 */
import { Search } from "lucide-react";
import { useState, type ReactElement, type RefObject } from "react";

import { Input } from "@/components/ui/input";
import { useDebouncedCommit } from "@/lib/hooks/use-debounced-commit";

import { FILTER_DEBOUNCE_MS, MIN_Q_LEN, type EditTokens } from "./filter-commit";

type QuickSearchProps = {
  value: string;
  resetToken: string;
  lastEdit: RefObject<EditTokens | null>;
  clearGen: RefObject<number>;
  onCommit: (q: string | undefined) => void;
};

export function QuickSearch({
  value,
  resetToken,
  lastEdit,
  clearGen,
  onCommit,
}: Readonly<QuickSearchProps>): ReactElement {
  // The generation is captured when the draft is EDITED, not per render:
  // a clearing write bumps it after the edit, and the fire-time check must
  // veto the stale draft even if an unrelated re-render lands between the
  // bump and the due timer (same rationale as the rail's useDraftSlice).
  const [draftState, setDraftState] = useState(() => ({ value, gen: clearGen.current }));
  const [synced, setSynced] = useState({ value, resetToken });
  // Reset the input to reflect `value` when the committed `q` changes from
  // outside (navigation) OR when any other filter changes (a section clear,
  // clear-all). The second case discards an uncommitted draft so a pending
  // keystroke cannot resurrect after the user clears filters; when it is
  // an own edit in another editing surface, the draft survives instead.
  if (value !== synced.value || resetToken !== synced.resetToken) {
    const editElsewhere = value === synced.value && lastEdit.current?.reset === resetToken;
    setSynced({ value, resetToken });
    if (!editElsewhere) setDraftState({ value, gen: clearGen.current });
  }
  const draft = draftState.value;
  const trimmed = draft.trim();
  const next = trimmed.length >= MIN_Q_LEN ? trimmed : undefined;
  // Treating an empty box (`next` undefined) as equal to an absent `q`
  // (`value` "") stops an untouched search from committing spuriously.
  useDebouncedCommit(
    next ?? "",
    value,
    () => {
      onCommit(next);
    },
    FILTER_DEBOUNCE_MS,
    () => clearGen.current !== draftState.gen,
  );

  return (
    <div className="relative min-w-0 flex-1">
      <Search
        aria-hidden="true"
        className="text-fg-muted pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2"
      />
      <Input
        type="search"
        aria-label="Search your library"
        placeholder="Search your library"
        className="h-9 pl-9"
        value={draft}
        onChange={(event) => {
          setDraftState({ value: event.target.value, gen: clearGen.current });
        }}
      />
    </div>
  );
}
