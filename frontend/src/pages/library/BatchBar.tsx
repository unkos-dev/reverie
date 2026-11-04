/**
 * Sticky batch-action bar, shown while table rows are selected: the
 * selection count, a shelf picker, and clear. Add to shelf is an awaited
 * serial mutation over the submitted selection (the shelves API is per-item).
 * Remove is deliberately absent: no delete contract exists.
 *
 * The count reflects loaded rows only (select-all cannot reach unloaded
 * pages under keyset paging); when more pages exist the note says so
 * rather than letting "all" be misread as "everything matching".
 */
import { FolderPlus, Loader2 } from "lucide-react";
import { useEffect, type ReactElement } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Group } from "react-aria-components";

import { addShelfItem, listShelves, type Shelf } from "@/api";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem } from "@/components/ui/dropdown-menu";
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
}: Readonly<BatchBarProps>): ReactElement | null {
  const queryClient = useQueryClient();
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

  const batch = useMutation({
    mutationFn: async ({
      shelf,
      ids,
    }: {
      shelf: Shelf;
      ids: readonly string[];
      retry?: boolean;
    }) => {
      const succeeded: string[] = [];
      const failed: string[] = [];
      for (const id of ids) {
        try {
          await addShelfItem(shelf.id, id);
          succeeded.push(id);
        } catch (error) {
          failed.push(id);
          console.error("[BatchBar] add to shelf failed", { id, error });
        }
      }
      return { succeeded, failed };
    },
    onSuccess: ({ succeeded }) => {
      if (succeeded.length > 0)
        void queryClient.invalidateQueries({ queryKey: queryKeys.shelves.all });
      onCompleted(succeeded);
    },
  });

  if (selectedIds.size === 0 && batch.isIdle) return null;

  function shelfStatusText(): string {
    if (shelvesError) return "Couldn't load shelves";
    if (shelves === undefined) return "Loading…";
    return "No shelves yet";
  }

  return (
    <Group
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
      {batch.isPending ? (
        <p role="status" className="text-fg-muted text-sm">
          Adding {batch.variables.ids.length} books to {batch.variables.shelf.name}…
        </p>
      ) : null}
      {batch.isSuccess ? (
        <p role="status" className="text-fg-muted text-sm">
          Added {batch.data.succeeded.length} of {batch.variables.ids.length} books to{" "}
          {batch.variables.shelf.name}.
          {batch.data.failed.length > 0
            ? ` ${String(batch.data.failed.length)} could not be added.`
            : ""}
        </p>
      ) : null}
      {(batch.isSuccess && batch.data.failed.length > 0) ||
      (batch.isPending && batch.variables.retry === true) ? (
        <Button
          type="button"
          variant="outline"
          size="sm"
          isPending={batch.isPending}
          onPress={() => {
            if (batch.isSuccess)
              batch.mutate({ shelf: batch.variables.shelf, ids: batch.data.failed, retry: true });
          }}
        >
          Retry failed books
        </Button>
      ) : null}
      {!batch.isIdle && !batch.isPending ? (
        <Button
          type="button"
          variant="link"
          size="xs"
          onPress={() => {
            batch.reset();
          }}
        >
          Dismiss result
        </Button>
      ) : null}
      <Button
        type="button"
        variant="link"
        size="xs"
        onPress={onClearSelection}
        className="text-fg-muted hover:text-fg focus-visible:ring-accent px-1 font-mono text-[0.65rem] uppercase tracking-[0.08em] underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2"
      >
        Clear selection
      </Button>
      <DropdownMenu>
        <Button
          type="button"
          variant="outline"
          size="sm"
          isDisabled={batch.isPending || selectedIds.size === 0}
        >
          {batch.isPending ? (
            <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          ) : (
            <FolderPlus className="size-4" aria-hidden="true" />
          )}
          Add to shelf
        </Button>
        <DropdownMenuContent placement="bottom end">
          {shelves === undefined || shelves.length === 0 ? (
            <DropdownMenuItem isDisabled>{shelfStatusText()}</DropdownMenuItem>
          ) : (
            shelves.map((shelf) => (
              <DropdownMenuItem
                key={shelf.id}
                onAction={() => {
                  batch.mutate({ shelf, ids: [...selectedIds] });
                }}
              >
                {shelf.name}
              </DropdownMenuItem>
            ))
          )}
        </DropdownMenuContent>
      </DropdownMenu>
    </Group>
  );
}
