/**
 * Production `/shelves` page (sub-phase 11d).
 *
 * Lists the caller's shelves with create / rename / delete. System
 * shelves render with disabled rename/delete affordances (the backend
 * 409s with `system-shelf-immutable` if the operator tries anyway).
 *
 * The sidebar integration into `/library` is a separate carry-over —
 * this page ships the CRUD surface so the rest of 11d can build on
 * it.
 */
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Pencil, Plus, Trash2 } from "lucide-react";
import { Suspense, useState, type ReactElement, type SyntheticEvent } from "react";
import { Link } from "react-router";
import { toast } from "sonner";

import { ApiError, createShelf, deleteShelf, listShelves, renameShelf, type Shelf } from "@/api";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { queryKeys } from "@/lib/query/keys";

/** Top-level page component (Suspense boundary + content). */
export function ShelvesListPage(): ReactElement {
  return (
    <Suspense fallback={<ShelvesSkeleton />}>
      <ShelvesContent />
    </Suspense>
  );
}

function ShelvesContent(): ReactElement {
  const qc = useQueryClient();
  const { data: shelves } = useSuspenseQuery<Shelf[]>({
    queryKey: queryKeys.shelves.list(),
    queryFn: ({ signal }) => listShelves(signal),
  });

  const [createOpen, setCreateOpen] = useState(false);
  const [renameTarget, setRenameTarget] = useState<Shelf | null>(null);

  const createMutation = useMutation({
    mutationFn: (name: string) => createShelf(name),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.shelves.all });
      toast.success("Shelf created.");
      setCreateOpen(false);
    },
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.detail : "Could not create shelf.");
    },
  });

  const renameMutation = useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => renameShelf(id, name),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.shelves.all });
      toast.success("Shelf renamed.");
      setRenameTarget(null);
    },
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.detail : "Could not rename shelf.");
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteShelf(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.shelves.all });
      toast.success("Shelf deleted.");
    },
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.detail : "Could not delete shelf.");
    },
  });

  return (
    <div className="mx-auto max-w-3xl px-6 py-10 sm:px-10">
      <header className="mb-8 flex items-center justify-between">
        <div>
          <h1 className="font-display text-3xl font-semibold tracking-tight text-fg">Shelves</h1>
          <p className="text-fg-muted mt-1 text-sm">
            {shelves.length} {shelves.length === 1 ? "shelf" : "shelves"}
          </p>
        </div>
        <Button
          type="button"
          onClick={() => {
            setCreateOpen(true);
          }}
        >
          <Plus className="size-4" aria-hidden="true" />
          New shelf
        </Button>
      </header>

      <ul className="space-y-2">
        {shelves.map((shelf) => (
          <li
            key={shelf.id}
            className="border-border bg-surface-1 flex items-center justify-between gap-3 rounded-md border p-3"
          >
            <Link
              to={`/shelves/${shelf.id}`}
              className="min-w-0 flex-1 truncate font-medium text-fg hover:underline"
            >
              {shelf.name}
            </Link>
            <span className="text-fg-muted text-xs">
              {shelf.item_count} {shelf.item_count === 1 ? "item" : "items"}
            </span>
            {shelf.is_system ? (
              <span className="text-fg-muted border-border rounded-full border px-2 py-0.5 text-xs">
                System
              </span>
            ) : (
              <>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  aria-label="Rename shelf"
                  onClick={() => {
                    setRenameTarget(shelf);
                  }}
                >
                  <Pencil className="size-4" aria-hidden="true" />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  aria-label="Delete shelf"
                  onClick={() => {
                    deleteMutation.mutate(shelf.id);
                  }}
                  disabled={deleteMutation.isPending}
                >
                  <Trash2 className="size-4" aria-hidden="true" />
                </Button>
              </>
            )}
          </li>
        ))}
      </ul>

      <NameDialog
        open={createOpen}
        title="New shelf"
        onClose={() => {
          setCreateOpen(false);
        }}
        onSubmit={(name) => {
          createMutation.mutate(name);
        }}
        pending={createMutation.isPending}
      />
      <NameDialog
        open={renameTarget !== null}
        title="Rename shelf"
        initialName={renameTarget?.name ?? ""}
        onClose={() => {
          setRenameTarget(null);
        }}
        onSubmit={(name) => {
          if (renameTarget !== null) {
            renameMutation.mutate({ id: renameTarget.id, name });
          }
        }}
        pending={renameMutation.isPending}
      />
    </div>
  );
}

interface NameDialogProps {
  open: boolean;
  title: string;
  initialName?: string;
  pending: boolean;
  onClose: () => void;
  onSubmit: (name: string) => void;
}

function NameDialog({
  open,
  title,
  initialName = "",
  pending,
  onClose,
  onSubmit,
}: NameDialogProps): ReactElement {
  const [name, setName] = useState(initialName);
  function handleSubmit(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    const trimmed = name.trim();
    if (trimmed.length === 0) return;
    onSubmit(trimmed);
  }
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="shelf-name">Name</Label>
            <Input
              id="shelf-name"
              autoFocus
              value={name}
              onChange={(e) => {
                setName(e.target.value);
              }}
            />
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={pending}>
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function ShelvesSkeleton(): ReactElement {
  return (
    <div className="mx-auto max-w-3xl px-6 py-10 sm:px-10">
      <Skeleton className="h-8 w-1/3" />
      <div className="mt-8 space-y-2">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-12 w-full" />
        ))}
      </div>
    </div>
  );
}
