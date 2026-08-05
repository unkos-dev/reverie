/**
 * Sticky batch-action bar, shown while table rows are selected: the
 * selection count, a shelf picker, and clear. Add to shelf is an awaited
 * serial loop over the selection (the shelves API is per-item; a burst of
 * parallel posts would hammer the backend), guarded against re-entry.
 * Remove is deliberately absent: no delete contract exists.
 *
 * The count reflects loaded rows only (select-all cannot reach unloaded
 * pages under keyset paging); when more pages exist the note says so
 * rather than letting "all" be misread as "everything matching".
 */
import { FolderPlus, Loader2 } from "lucide-react";
import { useEffect, useState, type ReactElement } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { addShelfItem, listShelves, type Shelf } from "@/api";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { queryKeys } from "@/lib/query/keys";

type BatchBarProps = {
  selectedIds: ReadonlySet<string>;
  /** True when unloaded pages exist, so "selected" cannot mean "all matching". */
  hasMorePages: boolean;
  onClearSelection: () => void;
  /** Drops the given ids from the selection (after a successful add). */
  onCompleted: (succeededIds: readonly string[]) => void;
};

export function BatchBar({
  selectedIds,
  hasMorePages,
  onClearSelection,
  onCompleted,
}: BatchBarProps): ReactElement | null {
  const queryClient = useQueryClient();
  const [running, setRunning] = useState(false);
  const {
    data: shelves,
    isError: shelvesError,
    error: shelvesQueryError,
  } = useQuery<Shelf[]>({
    queryKey: queryKeys.shelves.list(),
    queryFn: ({ signal }) => listShelves(signal),
    staleTime: 60_000,
  });
  // A shelf-less picker is sanctioned degradation, but the failure must
  // stay observable: the central QueryCache.onError only routes 401s, so
  // a non-auth failure would otherwise vanish.
  useEffect(() => {
    if (shelvesError) console.error("[BatchBar] shelves fetch failed", shelvesQueryError);
  }, [shelvesError, shelvesQueryError]);

  if (selectedIds.size === 0) return null;

  async function addAllToShelf(shelf: Shelf): Promise<void> {
    // Re-entry guard: a second pick while a run is in flight would double-post.
    if (running) return;
    setRunning(true);
    const ids = [...selectedIds];
    const succeeded: string[] = [];
    let failed = 0;
    // Serial on purpose; see the module docstring.
    for (const id of ids) {
      try {
        await addShelfItem(shelf.id, id);
        succeeded.push(id);
      } catch (error) {
        failed += 1;
        console.error("[BatchBar] add to shelf failed", { id, error });
      }
    }
    setRunning(false);
    if (succeeded.length > 0) {
      // Whole family: the rail's list counts AND any cached shelf detail
      // (membership list) are both stale after a membership write.
      void queryClient.invalidateQueries({ queryKey: queryKeys.shelves.all });
    }
    if (failed === 0) {
      toast.success(
        `Added ${String(succeeded.length)} ${succeeded.length === 1 ? "book" : "books"} to ${shelf.name}`,
      );
    } else {
      // Partial failure keeps the failed ids selected for a retry.
      toast.error(
        `Added ${String(succeeded.length)} of ${String(ids.length)} to ${shelf.name}; the rest stay selected`,
      );
    }
    onCompleted(succeeded);
  }

  return (
    <div
      role="toolbar"
      aria-label="Batch actions"
      className="border-border bg-surface-1 sticky bottom-4 z-10 mt-4 flex flex-wrap items-center gap-3 border p-3"
    >
      <strong className="text-fg text-sm font-medium" aria-live="polite">
        {selectedIds.size} selected
      </strong>
      {hasMorePages ? (
        <span className="text-fg-muted font-mono text-[0.65rem] uppercase tracking-[0.08em]">
          Selection covers loaded books only
        </span>
      ) : null}
      <span className="flex-1" />
      <button
        type="button"
        onClick={onClearSelection}
        className="text-fg-muted hover:text-fg focus-visible:ring-accent px-1 font-mono text-[0.65rem] uppercase tracking-[0.08em] underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2"
      >
        Clear selection
      </button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button type="button" variant="outline" size="sm" disabled={running}>
            {running ? (
              <Loader2 className="size-4 animate-spin" aria-hidden="true" />
            ) : (
              <FolderPlus className="size-4" aria-hidden="true" />
            )}
            Add to shelf
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {shelves === undefined || shelves.length === 0 ? (
            <DropdownMenuItem disabled>
              {shelvesError
                ? "Couldn't load shelves"
                : shelves === undefined
                  ? "Loading…"
                  : "No shelves yet"}
            </DropdownMenuItem>
          ) : (
            shelves.map((shelf) => (
              <DropdownMenuItem
                key={shelf.id}
                onSelect={() => {
                  void addAllToShelf(shelf);
                }}
              >
                {shelf.name}
              </DropdownMenuItem>
            ))
          )}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
