/**
 * Right-side book-detail drawer, opened by table-row activation. One of
 * the two mutually exclusive library overlays (the other is the filter
 * drawer); the container's single overlay slot enforces the exclusion,
 * and the Sheet primitive supplies the shared scrim, Escape handling,
 * and focus return.
 *
 * Actions are real product actions only: View full record navigates to
 * the book route; Add to shelf posts through the shelves API. Anything
 * without a backend contract (delete, download) is absent by decision.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, FolderPlus, Loader2 } from "lucide-react";
import { useEffect, type ReactElement } from "react";
import { Link } from "react-router";
import { toast } from "sonner";

import { addShelfItem, getBook, listShelves, type BookDetail, type Shelf } from "@/api";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { Skeleton } from "@/components/ui/skeleton";
import { queryKeys } from "@/lib/query/keys";

const EMPTY_CELL = "—";

type BookDetailDrawerProps = {
  /** Book to show; `null` renders the drawer closed. */
  bookId: string | null;
  onOpenChange: (open: boolean) => void;
};

export function BookDetailDrawer({ bookId, onOpenChange }: BookDetailDrawerProps): ReactElement {
  return (
    <Sheet open={bookId !== null} onOpenChange={onOpenChange}>
      <SheetContent
        side="right"
        aria-describedby={undefined}
        className="w-[390px] max-w-[100vw] overflow-y-auto"
      >
        {bookId === null ? null : <DrawerBody bookId={bookId} />}
      </SheetContent>
    </Sheet>
  );
}

function DrawerBody({ bookId }: { bookId: string }): ReactElement {
  const {
    data: book,
    isPending,
    isError,
    refetch,
  } = useQuery<BookDetail>({
    queryKey: queryKeys.books.detail(bookId),
    queryFn: ({ signal }) => getBook(bookId, signal),
  });

  if (isPending) {
    return (
      <div className="flex flex-col gap-4 p-6" aria-busy="true">
        <SheetHeader className="p-0">
          <SheetTitle className="sr-only">Book details</SheetTitle>
        </SheetHeader>
        <div className="flex gap-4">
          <Skeleton className="h-24 w-[68px] shrink-0" />
          <div className="flex min-w-0 flex-1 flex-col gap-2">
            <Skeleton className="h-4 w-20" />
            <Skeleton className="h-7 w-full" />
            <Skeleton className="h-4 w-2/3" />
          </div>
        </div>
        <Skeleton className="h-40 w-full" />
      </div>
    );
  }

  if (isError) {
    return (
      <div className="flex flex-col gap-3 p-6">
        <SheetHeader className="p-0">
          <SheetTitle>Book details</SheetTitle>
        </SheetHeader>
        <p role="alert" className="text-fg-muted text-sm">
          Couldn&apos;t load this book&apos;s details.
        </p>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => {
            void refetch();
          }}
        >
          Retry
        </Button>
      </div>
    );
  }

  const seriesText =
    book.series === null
      ? EMPTY_CELL
      : book.series.position === null
        ? book.series.name
        : `${book.series.name} · #${String(book.series.position)}`;
  const addedText = formatDate(book.created_at);

  return (
    <div className="flex flex-col">
      <div className="border-border bg-surface-1 border-b p-6">
        <SheetHeader className="mb-4 p-0">
          <p className="text-fg-muted font-mono text-[0.65rem] uppercase tracking-[0.12em]">
            Book details
          </p>
        </SheetHeader>
        <div className="flex gap-4">
          {book.cover_url !== "" ? (
            <img
              src={book.cover_url}
              alt=""
              decoding="async"
              className="border-border h-24 w-[68px] shrink-0 border object-cover"
            />
          ) : (
            <span
              aria-hidden="true"
              className="border-accent text-accent-text font-display flex h-24 w-[68px] shrink-0 items-center justify-center border text-center text-base leading-none"
            >
              {book.title.slice(0, 3).toUpperCase()}
            </span>
          )}
          <div className="min-w-0">
            <SheetTitle className="font-display text-fg text-2xl font-medium leading-tight tracking-tight">
              {book.title}
            </SheetTitle>
            <SheetDescription className="text-fg-muted mt-2 text-sm">
              {book.authors.length > 0 ? book.authors.join(", ") : EMPTY_CELL}
            </SheetDescription>
          </div>
        </div>
      </div>
      <div className="p-6">
        <dl className="border-border border-t">
          <DetailRow label="Series" value={seriesText} />
          <DetailRow label="Added" value={addedText} />
          <DetailRow label="Publisher" value={book.publisher ?? EMPTY_CELL} />
          <DetailRow label="ISBN" value={book.isbn_13 ?? EMPTY_CELL} />
          <DetailRow label="Language" value={book.language ?? EMPTY_CELL} />
        </dl>
        {book.description !== null ? (
          <p className="text-fg-muted mt-4 line-clamp-6 text-sm leading-relaxed">
            {book.description}
          </p>
        ) : null}
        <div className="mt-6 grid gap-2">
          <Button asChild variant="outline">
            <Link to={`/b/${book.id}`}>
              <ExternalLink className="size-4" aria-hidden="true" />
              View full record
            </Link>
          </Button>
          <AddToShelf bookId={book.id} bookTitle={book.title} />
        </div>
      </div>
    </div>
  );
}

function DetailRow({ label, value }: { label: string; value: string }): ReactElement {
  return (
    <div className="border-border grid grid-cols-[86px_1fr] gap-3 border-b py-3">
      <dt className="text-fg-muted font-mono text-[0.65rem] uppercase leading-5 tracking-[0.1em]">
        {label}
      </dt>
      <dd className="text-fg m-0 text-sm">{value}</dd>
    </div>
  );
}

function formatDate(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? EMPTY_CELL : date.toLocaleDateString();
}

/**
 * Shelf picker over the shelves list; each pick posts one membership row.
 * The shelves list is invalidated on success so the left rail's per-shelf
 * counts stay honest.
 */
function AddToShelf({ bookId, bookTitle }: { bookId: string; bookTitle: string }): ReactElement {
  const queryClient = useQueryClient();
  const {
    data: shelves,
    isError,
    error,
  } = useQuery<Shelf[]>({
    queryKey: queryKeys.shelves.list(),
    queryFn: ({ signal }) => listShelves(signal),
    staleTime: 60_000,
  });
  useEffect(() => {
    if (isError) console.error("[BookDetailDrawer] shelves fetch failed", error);
  }, [isError, error]);

  const addMutation = useMutation({
    mutationFn: ({ shelfId }: { shelfId: string; shelfName: string }) =>
      addShelfItem(shelfId, bookId),
    onSuccess: (_data, { shelfName }) => {
      toast.success(`Added "${bookTitle}" to ${shelfName}`);
      void queryClient.invalidateQueries({ queryKey: queryKeys.shelves.list() });
    },
    onError: (mutationError, { shelfName }) => {
      console.error("[BookDetailDrawer] add to shelf failed", mutationError);
      toast.error(`Couldn't add "${bookTitle}" to ${shelfName}`);
    },
  });

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button type="button" variant="outline" disabled={addMutation.isPending}>
          {addMutation.isPending ? (
            <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          ) : (
            <FolderPlus className="size-4" aria-hidden="true" />
          )}
          Add to shelf
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        {shelves === undefined || shelves.length === 0 ? (
          <DropdownMenuItem disabled>
            {isError
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
                addMutation.mutate({ shelfId: shelf.id, shelfName: shelf.name });
              }}
            >
              {shelf.name}
            </DropdownMenuItem>
          ))
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
