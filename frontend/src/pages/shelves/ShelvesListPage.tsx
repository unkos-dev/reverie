/**
 * Production `/shelves` page.
 *
 * Lists the caller's shelves with create / rename / delete. System
 * shelves render with disabled rename/delete affordances; the backend
 * rejects mutations on `is_system = TRUE` shelves with HTTP 409
 * `system-shelf-immutable`.
 */
import {
  usePreloadedQuery,
  useQueryLoader,
  useRelayEnvironment,
  type PreloadedQuery,
} from "react-relay";
import { ConnectionHandler, commitLocalUpdate } from "relay-runtime";
import { encodeGraphQLId, executeEditingOnce } from "@/api/graphql";
import { useShelves } from "@/api/shelves";
import { readerShelvesQuery } from "@/lib/relay/reader";
import {
  mutationsCreateShelfMutation,
  mutationsRenameShelfMutation,
  mutationsDeleteShelfMutation,
} from "@/lib/relay/mutations";
import type { readerShelvesQuery as ShelvesQuery } from "@/lib/relay/__generated__/readerShelvesQuery.graphql";
import type { mutationsCreateShelfMutation as CreateShelf } from "@/lib/relay/__generated__/mutationsCreateShelfMutation.graphql";
import type { mutationsRenameShelfMutation as RenameShelf } from "@/lib/relay/__generated__/mutationsRenameShelfMutation.graphql";
import type { mutationsDeleteShelfMutation as DeleteShelf } from "@/lib/relay/__generated__/mutationsDeleteShelfMutation.graphql";
import { Pencil, Plus, Trash2 } from "lucide-react";
import { useEffect, Suspense, useActionState, useRef, useState, type ReactElement } from "react";
import { Form, TextField, Input as AriaInput, Label as AriaLabel } from "react-aria-components";
import { z } from "zod";
import { Link, useLoaderData } from "react-router";

import { ApiError, type Shelf } from "@/api";
import { Button } from "@/components/ui/button";
import { DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { FieldError } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";

import type { loader } from "@/routes/shelves";

/** Top-level page component (Suspense boundary + content). */
export function ShelvesListPage(): ReactElement | null {
  const prepared = useLoaderData<typeof loader>();
  if (prepared === null) return null;
  return (
    <Suspense fallback={<ShelvesSkeleton />}>
      <ShelvesContent reference={prepared.reference} />
    </Suspense>
  );
}

function ShelvesContent({
  reference,
}: Readonly<{ reference: PreloadedQuery<ShelvesQuery> }>): ReactElement {
  const [current] = useQueryLoader<ShelvesQuery>(readerShelvesQuery, reference);
  const query = usePreloadedQuery<ShelvesQuery>(readerShelvesQuery, current ?? reference);
  const environment = useRelayEnvironment();
  const shelvesQuery = useShelves(query);
  const { data: shelves, error } = shelvesQuery;
  const controller = useRef<AbortController | null>(null);
  useEffect(() => {
    const lifetime = new AbortController();
    controller.current = lifetime;
    return () => {
      lifetime.abort();
    };
  }, []);
  const [createOpen, setCreateOpen] = useState(false);
  const [renameTarget, setRenameTarget] = useState<Shelf | null>(null);
  const [createPending, setCreatePending] = useState(false);
  const [renamePending, setRenamePending] = useState(false);
  const [deletion, setDeletion] = useState<{
    shelf: Shelf;
    pending: boolean;
    error: Error | null;
  }>();
  if (error !== null) throw error;
  async function create(name: string): Promise<void> {
    setCreatePending(true);
    try {
      const response = await executeEditingOnce<CreateShelf>(
        environment,
        mutationsCreateShelfMutation,
        { name },
        controller.current?.signal,
      );
      if (response !== undefined) {
        shelvesQuery.refetch();
        setCreateOpen(false);
      }
      setCreatePending(false);
    } catch (error_) {
      setCreatePending(false);
      shelvesQuery.refetch();
      throw error_;
    }
  }
  async function rename(name: string): Promise<void> {
    if (renameTarget === null) return;
    setRenamePending(true);
    try {
      const response = await executeEditingOnce<RenameShelf>(
        environment,
        mutationsRenameShelfMutation,
        { shelfId: encodeGraphQLId("Shelf", renameTarget.id), name },
        controller.current?.signal,
      );
      if (response !== undefined) {
        shelvesQuery.refetch();
        setRenameTarget(null);
      }
      setRenamePending(false);
    } catch (error_) {
      setRenamePending(false);
      shelvesQuery.refetch();
      throw error_;
    }
  }
  async function remove(shelf: Shelf): Promise<void> {
    setDeletion({ shelf, pending: true, error: null });
    try {
      const response = await executeEditingOnce<DeleteShelf>(
        environment,
        mutationsDeleteShelfMutation,
        { shelfId: encodeGraphQLId("Shelf", shelf.id) },
        controller.current?.signal,
      );
      if (response === undefined) return;
      commitLocalUpdate(environment, (store) => {
        const connection = ConnectionHandler.getConnection(
          store.getRoot(),
          "readerShelves_shelves",
        );
        if (connection != null)
          ConnectionHandler.deleteNode(connection, response.deleteShelf.shelfId);
        store.delete(response.deleteShelf.shelfId);
      });
      setDeletion(undefined);
    } catch (error_) {
      if (controller.current?.signal.aborted) return;
      setDeletion({
        shelf,
        pending: false,
        error: error_ instanceof Error ? error_ : new Error("Request failed."),
      });
      shelvesQuery.refetch();
    }
  }
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
          onPress={() => {
            setCreateOpen(true);
          }}
        >
          <Plus className="size-4" aria-hidden="true" />
          New shelf
        </Button>
      </header>

      {deletion?.error ? (
        <FieldError>
          Could not delete {deletion.shelf.name}: {deletion.error.message}
        </FieldError>
      ) : null}
      <ul className="space-y-2">
        {shelves.map((shelf) => (
          <li
            key={shelf.id}
            className="border-border bg-surface flex items-center justify-between gap-3 rounded-md border p-3"
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
                  onPress={() => {
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
                  onPress={() => {
                    void remove(shelf);
                  }}
                  isPending={deletion?.pending === true && deletion.shelf.id === shelf.id}
                  isDisabled={deletion?.pending === true && deletion.shelf.id !== shelf.id}
                >
                  <Trash2 className="size-4" aria-hidden="true" />
                </Button>
              </>
            )}
          </li>
        ))}
      </ul>

      <NameDialog
        key={createOpen ? "create-open" : "create-closed"}
        open={createOpen}
        title="New shelf"
        onClose={() => {
          setCreateOpen(false);
        }}
        onSubmit={create}
        pending={createPending}
      />
      <NameDialog
        // `key` forces remount when the rename target changes — without
        // it, `useState(initialName)` retains the previous shelf's
        // name on the next open (React only runs the initializer once
        // per mount).
        key={renameTarget?.id ?? "none"}
        open={renameTarget !== null}
        title="Rename shelf"
        initialName={renameTarget?.name ?? ""}
        onClose={() => {
          setRenameTarget(null);
        }}
        onSubmit={rename}
        pending={renamePending}
      />
    </div>
  );
}

type NameDialogProps = {
  open: boolean;
  title: string;
  initialName?: string;
  pending: boolean;
  onClose: () => void;
  onSubmit: (name: string) => Promise<void>;
};

function NameDialog({
  open,
  title,
  initialName = "",
  pending,
  onClose,
  onSubmit,
}: Readonly<NameDialogProps>): ReactElement {
  const [state, action, actionPending] = useActionState(
    async (_previous: { name: string; error: string | null }, data: FormData) => {
      const name = z.string().parse(data.get("name"));
      const trimmed = name.trim();
      if (trimmed.length === 0 || pending) return { name, error: null };
      try {
        await onSubmit(trimmed);
        return { name: initialName, error: null };
      } catch (err) {
        return { name, error: err instanceof ApiError ? err.detail : "Could not save shelf." };
      }
    },
    { name: initialName, error: null },
  );
  return (
    <DialogContent
      isOpen={open}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <DialogHeader>
        <DialogTitle>{title}</DialogTitle>
      </DialogHeader>
      <Form action={action} validationErrors={{}} className="space-y-4">
        <TextField name="name" defaultValue={state.name} className="space-y-2">
          <AriaLabel render={(props) => <Label {...props} />}>Name</AriaLabel>
          <AriaInput autoFocus render={(props) => <Input {...props} />} />
        </TextField>
        {state.error ? <FieldError>{state.error}</FieldError> : null}
        <DialogFooter>
          <Button type="button" variant="outline" onPress={onClose}>
            Cancel
          </Button>
          <Button type="submit" isPending={pending || actionPending}>
            Save
          </Button>
        </DialogFooter>
      </Form>
    </DialogContent>
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
