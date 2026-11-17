import { FolderPlus, Loader2, RefreshCw, X } from "lucide-react";
import type { ReactElement } from "react";
import { Group } from "react-aria-components";

import type { Shelf } from "@/api";
import type { ReadingStatus } from "@/api/books";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem } from "@/components/ui/dropdown-menu";

export type BatchAction =
  | { kind: "shelf"; shelf: Shelf }
  | { kind: "status"; status: ReadingStatus | null }
  | { kind: "refresh" };
export type BatchOutcome = {
  action: BatchAction;
  pending: boolean;
  retrying: boolean;
  total: number;
  succeeded: readonly string[];
  failed: readonly {
    id: string;
    action: "retry" | "reload" | "correct" | "none";
    message?: string;
  }[];
};
type BatchBarProps = {
  selectedIds: ReadonlySet<string>;
  hasMorePages: boolean;
  onClearSelection: () => void;
  shelves: readonly Shelf[] | undefined;
  shelvesError: boolean;
  canRefresh: boolean;
  result: BatchOutcome | undefined;
  onAction: (action: BatchAction) => void;
  onRetry: () => void;
  onDismiss: () => void;
};
const STATUS_OPTIONS: readonly { status: ReadingStatus | null; label: string }[] = [
  { status: "want_to_read", label: "Want to read" },
  { status: "reading", label: "Reading" },
  { status: "on_hold", label: "On hold" },
  { status: "finished", label: "Finished" },
  { status: "abandoned", label: "Abandoned" },
  { status: null, label: "No status" },
];

function batchStatus(result: BatchOutcome | undefined): string | undefined {
  if (result === undefined) return;
  const total = String(result.total);
  const succeeded = String(result.succeeded.length);
  const shelfName = result.action.kind === "shelf" ? result.action.shelf.name : "";
  const messages: Record<BatchAction["kind"], { pending: string; settled: string }> = {
    shelf: {
      pending: `Adding ${total} books to ${shelfName}…`,
      settled: `Added ${succeeded} of ${total} books to ${shelfName}.`,
    },
    status: {
      pending: `Setting status for ${total} books…`,
      settled: `Set status for ${succeeded} of ${total} books.`,
    },
    refresh: {
      pending: `Scheduling metadata refresh for ${total} books…`,
      settled: `Scheduled metadata refresh for ${succeeded} of ${total} books.`,
    },
  };
  const message = messages[result.action.kind];
  let status = result.pending ? message.pending : message.settled;
  if (!result.pending && result.failed.length > 0)
    status += ` ${String(result.failed.length)} could not be saved.`;
  return status;
}

export function BatchBar({
  selectedIds,
  hasMorePages,
  onClearSelection,
  shelves,
  shelvesError,
  canRefresh,
  result,
  onAction,
  onRetry,
  onDismiss,
}: Readonly<BatchBarProps>): ReactElement | null {
  if (selectedIds.size === 0 && result === undefined) return null;
  const pending = result?.pending === true;
  const status = batchStatus(result);
  let shelfStatus = "No shelves yet";
  if (shelvesError) shelfStatus = "Couldn't load shelves";
  else if (shelves === undefined) shelfStatus = "Loading…";
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
      {status === undefined ? null : (
        <p role="status" className="text-fg-muted text-sm">
          {status}
        </p>
      )}
      {result?.failed.some((failure) => failure.action === "retry") === true ||
      (result?.retrying === true && pending) ? (
        <Button type="button" variant="outline" size="sm" isPending={pending} onPress={onRetry}>
          Retry failed books
        </Button>
      ) : null}
      {result !== undefined && !pending ? (
        <Button type="button" variant="link" size="xs" onPress={onDismiss}>
          Dismiss result
        </Button>
      ) : null}
      <DropdownMenu>
        <Button type="button" variant="default" isDisabled={pending || selectedIds.size === 0}>
          {pending ? (
            <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          ) : (
            <FolderPlus className="size-4" aria-hidden="true" />
          )}
          Add to shelf
        </Button>
        <DropdownMenuContent placement="bottom end">
          {shelves === undefined || shelves.length === 0 ? (
            <DropdownMenuItem isDisabled>{shelfStatus}</DropdownMenuItem>
          ) : (
            shelves.map((shelf) => (
              <DropdownMenuItem
                key={shelf.id}
                onAction={() => {
                  onAction({ kind: "shelf", shelf });
                }}
              >
                {shelf.name}
              </DropdownMenuItem>
            ))
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      <DropdownMenu>
        <Button type="button" variant="outline" isDisabled={pending || selectedIds.size === 0}>
          Set status
        </Button>
        <DropdownMenuContent placement="bottom end">
          {STATUS_OPTIONS.map(({ status: value, label }) => (
            <DropdownMenuItem
              key={value ?? "none"}
              onAction={() => {
                onAction({ kind: "status", status: value });
              }}
            >
              {label}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      {canRefresh ? (
        <Button
          type="button"
          variant="outline"
          isDisabled={pending || selectedIds.size === 0}
          onPress={() => {
            onAction({ kind: "refresh" });
          }}
        >
          <RefreshCw aria-hidden="true" className="size-4" />
          Refresh metadata
        </Button>
      ) : null}
      {result?.failed
        .filter((failure) => failure.message !== undefined)
        .map((failure) => (
          <p key={failure.id} className="text-danger-text text-sm">
            {failure.message}
          </p>
        ))}
      <span aria-hidden="true" className="bg-border mx-1 h-6 w-px" />
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label="Clear selection"
        isDisabled={pending}
        onPress={onClearSelection}
      >
        <X className="size-4" aria-hidden="true" />
      </Button>
    </Group>
  );
}
