/**
 * Production `/shelves` page.
 *
 * Lists the caller's shelves with create / rename / delete. System
 * shelves render with disabled rename/delete affordances; the backend
 * refuses changes to them with a `SystemShelfImmutable` result.
 */
import {
  graphql,
  useMutation,
  usePaginationFragment,
  usePreloadedQuery,
  useQueryLoader,
  type PreloadedQuery,
} from "react-relay";
import { ConnectionHandler, ROOT_ID } from "relay-runtime";
import type { ShelvesListPageQuery } from "@/lib/relay/__generated__/ShelvesListPageQuery.graphql";
import type {
  ShelvesListPage_shelves$data,
  ShelvesListPage_shelves$key,
} from "@/lib/relay/__generated__/ShelvesListPage_shelves.graphql";
import type { ShelvesListPagePaginationQuery } from "@/lib/relay/__generated__/ShelvesListPagePaginationQuery.graphql";
import type { ShelvesListPageCreateMutation } from "@/lib/relay/__generated__/ShelvesListPageCreateMutation.graphql";
import type { ShelvesListPageRenameMutation } from "@/lib/relay/__generated__/ShelvesListPageRenameMutation.graphql";
import type { ShelvesListPageDeleteMutation } from "@/lib/relay/__generated__/ShelvesListPageDeleteMutation.graphql";
import { Pencil, Plus, Trash2 } from "lucide-react";
import {
  Suspense,
  useActionState,
  useEffect,
  useState,
  useTransition,
  type ReactElement,
} from "react";
import { Form, TextField, Input as AriaInput, Label as AriaLabel } from "react-aria-components";
import { z } from "zod";
import { Link, useLoaderData } from "react-router";

import { Button } from "@/components/ui/button";
import { DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { FieldError } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";

import type { loader } from "@/routes/shelves";

const shelvesListPageQuery = graphql`
  query ShelvesListPageQuery @throwOnFieldError {
    ...ShelvesListPage_shelves
  }
`;

const shelvesFragment = graphql`
  fragment ShelvesListPage_shelves on Query
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 50 }, cursor: { type: "String" })
  @refetchable(queryName: "ShelvesListPagePaginationQuery") {
    shelves(first: $count, after: $cursor) @connection(key: "ShelvesListPage_shelves") {
      edges {
        node {
          id
          name
          isSystem
          itemCount
        }
      }
    }
  }
`;

const createShelfMutation = graphql`
  mutation ShelvesListPageCreateMutation($input: CreateShelfInput!, $connections: [ID!]!) {
    createShelf(input: $input) {
      __typename
      ... on ShelfCreated {
        edge @appendEdge(connections: $connections) {
          cursor
          node {
            id
            name
            isSystem
            itemCount
            createdAt
            updatedAt
          }
        }
      }
      ... on ShelfNameInvalid {
        message
      }
    }
  }
`;

const renameShelfMutation = graphql`
  mutation ShelvesListPageRenameMutation($input: RenameShelfInput!) {
    renameShelf(input: $input) {
      __typename
      ... on ShelfRenamed {
        shelf {
          id
          name
          updatedAt
        }
      }
      ... on ShelfNameInvalid {
        message
      }
      ... on SystemShelfImmutable {
        message
      }
      ... on NotFound {
        message
      }
    }
  }
`;

const deleteShelfMutation = graphql`
  mutation ShelvesListPageDeleteMutation($input: DeleteShelfInput!, $connections: [ID!]!) {
    deleteShelf(input: $input) {
      __typename
      ... on ShelfDeleted {
        deletedShelfId @deleteEdge(connections: $connections)
      }
      ... on SystemShelfImmutable {
        message
      }
      ... on NotFound {
        message
      }
    }
  }
`;

const SHELVES_CONNECTION = ConnectionHandler.getConnectionID(ROOT_ID, "ShelvesListPage_shelves");

type ShelfRow = ShelvesListPage_shelves$data["shelves"]["edges"][number]["node"];

/** Top-level page component (Suspense boundary + content). */
export function ShelvesListPage(): ReactElement | null {
  const prepared = useLoaderData<typeof loader>();
  const [reference] = useQueryLoader<ShelvesListPageQuery>(shelvesListPageQuery, prepared);
  if (reference == null || prepared === null) return null;
  return (
    <Suspense fallback={<ShelvesSkeleton />}>
      <ShelvesContent reference={reference} />
    </Suspense>
  );
}

function ShelvesContent({
  reference,
}: Readonly<{ reference: PreloadedQuery<ShelvesListPageQuery> }>): ReactElement {
  const query = usePreloadedQuery<ShelvesListPageQuery>(shelvesListPageQuery, reference);
  const { data, hasNext, isLoadingNext, loadNext, refetch } = usePaginationFragment<
    ShelvesListPagePaginationQuery,
    ShelvesListPage_shelves$key
  >(shelvesFragment, query);
  const [error, setError] = useState<Error | null>(null);
  const [, startTransition] = useTransition();
  useEffect(() => {
    if (hasNext && !isLoadingNext) loadNext(50, { onComplete: setError });
  }, [hasNext, isLoadingNext, loadNext]);
  const shelves = data.shelves.edges.map((edge) => edge.node);
  function reload(): void {
    startTransition(() => {
      refetch({}, { fetchPolicy: "network-only", onComplete: setError });
    });
  }
  const [commitCreate, createPending] =
    useMutation<ShelvesListPageCreateMutation>(createShelfMutation);
  const [commitRename, renamePending] =
    useMutation<ShelvesListPageRenameMutation>(renameShelfMutation);
  const [commitDelete, deletePending] =
    useMutation<ShelvesListPageDeleteMutation>(deleteShelfMutation);
  const [createOpen, setCreateOpen] = useState(false);
  const [renameId, setRenameId] = useState<string | null>(null);
  const renameTarget = shelves.find((shelf) => shelf.id === renameId);
  const [deletion, setDeletion] = useState<{ id: string; name: string; error: string | null }>();
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
          reload();
          reject(failure);
        },
      });
    });
  }
  function rename(name: string): Promise<string | null> {
    const target = renameTarget;
    if (target === undefined) return Promise.resolve(null);
    return new Promise((resolve, reject) => {
      commitRename({
        variables: { input: { id: target.id, name } },
        onCompleted: ({ renameShelf }) => {
          if (renameShelf.__typename === "ShelfRenamed") {
            setRenameId(null);
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
          reload();
          reject(failure);
        },
      });
    });
  }
  function remove({ id, name }: ShelfRow): void {
    setDeletion({ id, name, error: null });
    commitDelete({
      variables: { input: { id }, connections: [SHELVES_CONNECTION] },
      updater: (store, response) => {
        if (response?.deleteShelf.__typename === "ShelfDeleted")
          store.get(response.deleteShelf.deletedShelfId)?.invalidateRecord();
      },
      onCompleted: ({ deleteShelf }) => {
        if (deleteShelf.__typename === "ShelfDeleted") setDeletion(undefined);
        else
          setDeletion({
            id,
            name,
            error:
              deleteShelf.__typename === "SystemShelfImmutable" ||
              deleteShelf.__typename === "NotFound"
                ? deleteShelf.message
                : "Request failed.",
          });
      },
      onError: (failure) => {
        setDeletion({ id, name, error: failure.message });
        reload();
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
          Could not delete {deletion.name}: {deletion.error}
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
              {shelf.itemCount} {shelf.itemCount === 1 ? "item" : "items"}
            </span>
            {shelf.isSystem ? (
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
                    setRenameId(shelf.id);
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
                  isPending={deletePending && deletion?.id === shelf.id}
                  isDisabled={deletePending && deletion?.id !== shelf.id}
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
        open={renameTarget !== undefined}
        title="Rename shelf"
        initialName={renameTarget?.name ?? ""}
        onClose={() => {
          setRenameId(null);
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
