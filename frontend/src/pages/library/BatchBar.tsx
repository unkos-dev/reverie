import { FolderPlus, Loader2, X } from "lucide-react";
import type { ReactElement } from "react";
import { Group } from "react-aria-components";

import type { Shelf } from "@/api";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem } from "@/components/ui/dropdown-menu";

type BatchBarProps = {
  selectedIds: ReadonlySet<string>;
  /** True when unloaded pages exist, so "selected" cannot mean "all matching". */
  hasMorePages: boolean;
  onClearSelection: () => void;
  shelves: readonly Shelf[] | undefined;
  shelvesError: boolean;
  batch: {
    isIdle: boolean;
    isPending: boolean;
    isSuccess: boolean;
    variables: { shelf: Shelf; ids: readonly string[]; retry?: boolean } | undefined;
    data: { succeeded: string[]; failed: string[] } | undefined;
    mutate: (input: { shelf: Shelf; ids: readonly string[]; retry?: boolean }) => void;
    reset: () => void;
  };
};

export function BatchBar({
  selectedIds,
  hasMorePages,
  onClearSelection,
  shelves,
  shelvesError,
  batch,
}: Readonly<BatchBarProps>): ReactElement | null {
  if (selectedIds.size === 0 && batch.isIdle) return null;

  function shelfStatusText(): string {
    if (shelvesError) return "Couldn't load shelves";
    if (shelves === undefined) return "Loading…";
    return "No shelves yet";
  }

  return (
    <Group
      aria-label="Batch actions"
      className="border-hairline bg-(--bar) backdrop-blur-[22px] shadow-(--shadow-raised) sticky bottom-[calc(1rem+3rem+0.75rem+2px+0.75rem)] z-10 mt-4 flex flex-wrap items-center mx-auto w-fit justify-center gap-3 rounded-2xl border p-2 lg:static lg:shrink-0"
    >
      <strong className="text-fg text-sm font-medium" aria-live="polite">
        {selectedIds.size} selected
      </strong>
      {hasMorePages ? (
        <span className="text-fg-muted text-sm">Selection covers loaded books only</span>
      ) : null}
      {batch.isPending ? (
        <p role="status" className="text-fg-muted text-sm">
          Adding {batch.variables?.ids.length} books to {batch.variables?.shelf.name}…
        </p>
      ) : null}
      {batch.isSuccess ? (
        <p role="status" className="text-fg-muted text-sm">
          Added {batch.data?.succeeded.length} of {batch.variables?.ids.length} books to{" "}
          {batch.variables?.shelf.name}.
          {(batch.data?.failed.length ?? 0) > 0
            ? ` ${String(batch.data?.failed.length)} could not be added.`
            : ""}
        </p>
      ) : null}
      {(batch.isSuccess && (batch.data?.failed.length ?? 0) > 0) ||
      (batch.isPending && batch.variables?.retry === true) ? (
        <Button
          type="button"
          variant="outline"
          size="sm"
          isPending={batch.isPending}
          onPress={() => {
            if (batch.isSuccess && batch.variables !== undefined && batch.data !== undefined)
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
      <DropdownMenu>
        <Button
          type="button"
          variant="default"
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
      <span aria-hidden="true" className="bg-border mx-1 h-6 w-px" />
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label="Clear selection"
        onPress={onClearSelection}
        className="text-fg-muted"
      >
        <X aria-hidden="true" className="size-3.5" />
      </Button>
    </Group>
  );
}
