/**
 * Production `/library` page.
 *
 * Mirrors the visual contract of the dev hero (`/design/hero/library`)
 * — same tokens, same grid spacing, same typographic hierarchy — but
 * sources data from the real `/api/books` endpoint via react-query's
 * `useSuspenseInfiniteQuery`. The route loader has already seeded
 * page 1 into the cache; this component subscribes and renders.
 *
 * Visual filter affordances (shelf chips, search input, command
 * palette) are deferred to sub-phase 11b. This page renders the grid
 * / list toggle and the Load-more pagination control only.
 */
import { useInfiniteQuery, type InfiniteData } from "@tanstack/react-query";
import { LayoutGrid, List, Loader2 } from "lucide-react";
import { Suspense, type ReactElement } from "react";
import { Link, useSearchParams } from "react-router";

import { listBooks, type BookListItem, type BookListResponse } from "@/api";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { queryKeys } from "@/lib/query/keys";

import { paramsFromSearch } from "@/routes/library-params";

type ViewMode = "grid" | "list";

/**
 * Top-level page component. The `<Suspense>` boundary catches the
 * initial fetch (already prefetched by the loader, but the boundary
 * is required by `useSuspenseInfiniteQuery` semantics in failure /
 * cache-miss scenarios).
 */
export function LibraryPage(): ReactElement {
  return (
    <Suspense fallback={<LibrarySkeleton />}>
      <LibraryContent />
    </Suspense>
  );
}

function LibraryContent(): ReactElement {
  const [searchParams, setSearchParams] = useSearchParams();
  const viewMode: ViewMode = searchParams.get("view") === "list" ? "list" : "grid";
  const params = paramsFromSearch(searchParams);
  // Strip cursor from the cache key — Load more is driven by react-query's pageParam.
  const cacheParams = { ...params };
  delete cacheParams.cursor;

  const { data, fetchNextPage, hasNextPage, isFetchingNextPage } = useInfiniteQuery<
    BookListResponse,
    Error,
    InfiniteData<BookListResponse, string | undefined>,
    ReturnType<typeof queryKeys.books.list>,
    string | undefined
  >({
    queryKey: queryKeys.books.list(cacheParams),
    queryFn: ({ signal, pageParam }) =>
      listBooks(
        pageParam === undefined ? cacheParams : { ...cacheParams, cursor: pageParam },
        signal,
      ),
    initialPageParam: undefined,
    getNextPageParam: (lastPage) => lastPage.next_cursor ?? undefined,
  });

  const items: BookListItem[] = (data?.pages ?? []).flatMap((p) => p.items);

  function setView(next: ViewMode): void {
    const updated = new URLSearchParams(searchParams);
    if (next === "grid") updated.delete("view");
    else updated.set("view", next);
    setSearchParams(updated, { replace: true });
  }

  return (
    <div className="mx-auto max-w-7xl px-6 py-10 sm:px-10">
      <header className="mb-8 flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="font-display text-3xl font-semibold tracking-tight text-fg">Library</h1>
          <p className="text-fg-muted mt-1 text-sm">
            {items.length === 0
              ? "No books yet."
              : `${String(items.length)} ${items.length === 1 ? "book" : "books"}`}
          </p>
        </div>
        <div
          role="group"
          aria-label="View mode"
          className="border-border bg-surface-1 inline-flex items-center rounded-md border p-1"
        >
          <Button
            type="button"
            variant="ghost"
            size="sm"
            aria-pressed={viewMode === "grid"}
            onClick={() => {
              setView("grid");
            }}
            className={viewMode === "grid" ? "bg-accent-soft text-fg hover:bg-accent-soft" : ""}
          >
            <LayoutGrid className="size-4" aria-hidden="true" />
            <span className="sr-only">Grid</span>
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            aria-pressed={viewMode === "list"}
            onClick={() => {
              setView("list");
            }}
            className={viewMode === "list" ? "bg-accent-soft text-fg hover:bg-accent-soft" : ""}
          >
            <List className="size-4" aria-hidden="true" />
            <span className="sr-only">List</span>
          </Button>
        </div>
      </header>
      <Separator className="mb-8" />

      {items.length === 0 ? (
        <EmptyState />
      ) : viewMode === "grid" ? (
        <BookGrid items={items} />
      ) : (
        <BookList items={items} />
      )}

      {hasNextPage ? (
        <div className="mt-10 flex justify-center">
          <Button
            type="button"
            variant="outline"
            disabled={isFetchingNextPage}
            onClick={() => {
              void fetchNextPage();
            }}
          >
            {isFetchingNextPage ? (
              <>
                <Loader2 className="mr-2 size-4 animate-spin" aria-hidden="true" />
                Loading…
              </>
            ) : (
              "Load more"
            )}
          </Button>
        </div>
      ) : null}
    </div>
  );
}

interface BookGridProps {
  items: BookListItem[];
}

function BookGrid({ items }: BookGridProps): ReactElement {
  return (
    <ul
      data-testid="library-grid"
      className="grid grid-cols-2 gap-x-6 gap-y-8 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6"
    >
      {items.map((book) => (
        <li key={book.id}>
          <BookCard book={book} />
        </li>
      ))}
    </ul>
  );
}

interface BookListProps {
  items: BookListItem[];
}

function BookList({ items }: BookListProps): ReactElement {
  return (
    <table data-testid="library-list" className="border-border w-full border-collapse text-sm">
      <thead className="text-fg-muted text-left text-xs uppercase tracking-wide">
        <tr>
          <th scope="col" className="py-2 pr-4 font-medium">
            Title
          </th>
          <th scope="col" className="py-2 pr-4 font-medium">
            Author
          </th>
          <th scope="col" className="py-2 pr-4 font-medium">
            Series
          </th>
          <th scope="col" className="py-2 pr-4 font-medium">
            ISBN
          </th>
        </tr>
      </thead>
      <tbody>
        {items.map((book) => (
          <tr key={book.id} className="border-border hover:bg-surface-1 border-t">
            <td className="py-3 pr-4">
              <Link to={`/b/${book.id}`} className="hover:text-accent text-fg font-medium">
                {book.title}
              </Link>
            </td>
            <td className="text-fg-muted py-3 pr-4">{book.authors.join(", ")}</td>
            <td className="text-fg-muted py-3 pr-4">
              {book.series
                ? `${book.series.name}${
                    book.series.position !== null ? ` · #${String(book.series.position)}` : ""
                  }`
                : "—"}
            </td>
            <td className="text-fg-muted py-3 pr-4 font-mono text-xs">{book.isbn_13 ?? "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

interface BookCardProps {
  book: BookListItem;
}

function BookCard({ book }: BookCardProps): ReactElement {
  const seriesLabel =
    book.series && book.series.position !== null
      ? `${book.series.name} · #${String(book.series.position)}`
      : (book.series?.name ?? null);
  return (
    <article className="group">
      <Link
        to={`/b/${book.id}`}
        className="focus-visible:ring-accent focus-visible:ring-offset-canvas flex flex-col gap-3 rounded-md transition-transform duration-200 hover:-translate-y-0.5 focus-visible:-translate-y-0.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2"
      >
        <div className="border-border group-hover:border-border-strong bg-surface-1 relative aspect-[2/3] overflow-hidden rounded-md border transition-colors">
          <CoverImage src={book.cover_url} alt={`Cover of ${book.title}`} />
          {seriesLabel !== null ? (
            <span className="bg-canvas/85 text-fg border-border absolute left-2 top-2 rounded-sm border px-2 py-1 text-[0.62rem] uppercase tracking-[0.14em] backdrop-blur-sm">
              {seriesLabel}
            </span>
          ) : null}
        </div>
        <div className="flex flex-col gap-1">
          <h3 className="font-display text-fg line-clamp-2 text-sm font-semibold leading-tight">
            {book.title}
          </h3>
          <p className="text-fg-muted line-clamp-1 text-xs leading-tight">
            {book.authors.join(", ")}
          </p>
        </div>
      </Link>
    </article>
  );
}

interface CoverImageProps {
  src: string;
  alt: string;
}

function CoverImage({ src, alt }: CoverImageProps): ReactElement {
  return (
    <img src={src} alt={alt} loading="lazy" decoding="async" className="size-full object-cover" />
  );
}

function EmptyState(): ReactElement {
  return (
    <div className="border-border text-fg-muted flex min-h-[40vh] flex-col items-center justify-center rounded-md border border-dashed py-16 text-center">
      <p className="font-display text-fg mb-2 text-xl font-semibold">No books yet</p>
      <p className="text-sm">Once ingestion completes, books appear here.</p>
    </div>
  );
}

function LibrarySkeleton(): ReactElement {
  const PLACEHOLDERS = Array.from({ length: 12 }, (_, i) => i);
  return (
    <div className="mx-auto max-w-7xl px-6 py-10 sm:px-10" aria-busy="true">
      <Skeleton className="mb-4 h-9 w-48" />
      <Separator className="mb-8" />
      <div className="grid grid-cols-2 gap-x-6 gap-y-8 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
        {PLACEHOLDERS.map((i) => (
          <div key={i} className="flex flex-col gap-3">
            <Skeleton className="aspect-[2/3] w-full rounded-md" />
            <Skeleton className="h-4 w-3/4" />
            <Skeleton className="h-3 w-1/2" />
          </div>
        ))}
      </div>
    </div>
  );
}
