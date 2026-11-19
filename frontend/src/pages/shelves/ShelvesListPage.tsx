/**
 * Production `/shelves` page.
 *
 * Lists the caller's shelves with create / rename / delete. System
 * shelves render with disabled rename/delete affordances; the backend
 * refuses changes to them with a `SystemShelfImmutable` result.
 */
import { useMutation, usePreloadedQuery, useQueryLoader, type PreloadedQuery } from "react-relay";
import { ConnectionHandler, ROOT_ID } from "relay-runtime";
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
import { Suspense, useActionState, useState, type ReactElement } from "react";
import { Form, TextField, Input as AriaInput, Label as AriaLabel } from "react-aria-components";
import { z } from "zod";
import { Link, useLoaderData } from "react-router";

import { type Shelf } from "@/api";
import { Button } from "@/components/ui/button";
import { DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { FieldError } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";

import type { loader } from "@/routes/shelves";

const SHELVES_CONNECTION = ConnectionHandler.getConnectionID(ROOT_ID, "readerShelves_shelves");

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
  const shelvesQuery = useShelves(query);
  const { data: shelves, error } = shelvesQuery;
  const [commitCreate, createPending] = useMutation<CreateShelf>(mutationsCreateShelfMutation);
  const [commitRename, renamePending] = useMutation<RenameShelf>(mutationsRenameShelfMutation);
  const [commitDelete, deletePending] = useMutation<DeleteShelf>(mutationsDeleteShelfMutation);
  const [createOpen, setCreateOpen] = useState(false);
  const [renameTarget, setRenameTarget] = useState<Shelf | null>(null);
  const [deletion, setDeletion] = useState<{ shelf: Shelf; error: string | null }>();
  if (error !== null) throw error;
  function create(name: string): Promise<string | null> {
    return new Promise((resolve, reject) => {
      commitCreate({
        variables: { input: { name }, connections: [SHELVES_CONNECTION] },
        onCompleted: ({ createShelf }) => {
          if (createShelf.__typename === "ShelfCreated") {
            setCreateOpen(false);
            resolve(null);
          } else if (createShelf.__typename === "ShelfNameInvalid") {
            resolve(createShelf.message);
          } else {
            reject(new Error("Unexpected shelf result."));
          }
        },
        onError: (failure) => {
          shelvesQuery.refetch();
          reject(failure);
        },
      });
    });
  }
  function rename(name: string): Promise<string | null> {
    const target = renameTarget;
    if (target === null) return Promise.resolve(null);
    return new Promise((resolve, reject) => {
      commitRename({
        variables: { input: { id: target.id, name } },
        onCompleted: ({ renameShelf }) => {
          if (renameShelf.__typename === "ShelfRenamed") {
            setRenameTarget(null);
            resolve(null);
          } else if (
            renameShelf.__typename === "ShelfNameInvalid" ||
            renameShelf.__typename === "SystemShelfImmutable" ||
            renameShelf.__typename === "NotFound"
          ) {
            resolve(renameShelf.message);
          } else {
            reject(new Error("Unexpected shelf result."));
          }
        },
        onError: (failure) => {
          shelvesQuery.refetch();
          reject(failure);
        },
      });
    });
  }
  function remove(shelf: Shelf): void {
    setDeletion({ shelf, error: null });
    commitDelete({
      variables: { input: { id: shelf.id }, connections: [SHELVES_CONNECTION] },
      onCompleted: ({ deleteShelf }) => {
        if (deleteShelf.__typename === "ShelfDeleted") setDeletion(undefined);
        else
          setDeletion({
            shelf,
            error:
              deleteShelf.__typename === "SystemShelfImmutable" ||
              deleteShelf.__typename === "NotFound"
                ? deleteShelf.message
                : "Request failed.",
          });
      },
      onError: (failure) => {
        setDeletion({ shelf, error: failure.message });
        shelvesQuery.refetch();
      },
    });
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
          Could not delete {deletion.shelf.name}: {deletion.error}
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
                    remove(shelf);
                  }}
                  isPending={deletePending && deletion?.shelf.id === shelf.id}
                  isDisabled={deletePending && deletion?.shelf.id !== shelf.id}
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
  onSubmit: (name: string) => Promise<string | null>;
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
        const refusal = await onSubmit(trimmed);
        return refusal === null ? { name: initialName, error: null } : { name, error: refusal };
      } catch {
        return { name, error: "Could not save shelf." };
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
