/**
 * Production `/shelves/:id` page.
 *
 * Renders the shelf's items in the order the server returns them: when
 * each was added.
 */
import {
  useFragment,
  usePaginationFragment,
  usePreloadedQuery,
  useQueryLoader,
  type PreloadedQuery,
} from "react-relay";
import { decodeGraphQLId } from "@/api/graphql";
import { readerShelfQuery, readerShelf, readerShelfItems } from "@/lib/relay/reader";
import type { readerShelfQuery as ShelfQuery } from "@/lib/relay/__generated__/readerShelfQuery.graphql";
import type { readerShelf$key } from "@/lib/relay/__generated__/readerShelf.graphql";
import type { readerShelfItems$key } from "@/lib/relay/__generated__/readerShelfItems.graphql";
import type { readerShelfItemsPaginationQuery as ItemsPagination } from "@/lib/relay/__generated__/readerShelfItemsPaginationQuery.graphql";
import { useEffect, useState, Suspense, type ReactElement } from "react";
import { useLoaderData } from "react-router";

import { Skeleton } from "@/components/ui/skeleton";

import type { loader } from "@/routes/shelf-detail";

/** Top-level page component (Suspense boundary + content). */
export function ShelfDetailPage(): ReactElement | null {
  const prepared = useLoaderData<typeof loader>();
  if (prepared === null) return null;
  return (
    <Suspense fallback={<ShelfDetailSkeleton />}>
      <ShelfDetailContent reference={prepared.reference} />
    </Suspense>
  );
}

function ShelfDetailContent({
  reference,
}: {
  reference: PreloadedQuery<ShelfQuery>;
}): ReactElement {
  const [current] = useQueryLoader<ShelfQuery>(readerShelfQuery, reference);
  const query = usePreloadedQuery<ShelfQuery>(readerShelfQuery, current ?? reference);
  const data = useFragment<readerShelf$key>(readerShelf, query.shelf);
  const {
    data: items,
    hasNext,
    isLoadingNext,
    loadNext,
  } = usePaginationFragment<ItemsPagination, readerShelfItems$key>(readerShelfItems, data);
  const [error, setError] = useState<Error | null>(null);
  useEffect(() => {
    if (hasNext && !isLoadingNext) loadNext(50, { onComplete: setError });
  }, [hasNext, isLoadingNext, loadNext]);
  if (error !== null) throw error;
  if (data == null) throw new Error("Shelf not found.");
  return (
    <div className="mx-auto max-w-3xl px-6 py-10 sm:px-10">
      <header className="mb-6">
        <p className="text-fg-muted text-xs uppercase tracking-wider">Shelf</p>
        <h1 className="font-display mt-1 text-3xl font-semibold tracking-tight text-fg">
          {data.name}
        </h1>
        <p className="text-fg-muted mt-2 text-sm">
          {data.itemCount} {data.itemCount === 1 ? "item" : "items"}
        </p>
      </header>
      <ul className="space-y-2">
        {items?.items.edges.map(({ node }) => (
          <ShelfItemRow key={node.id} id={decodeGraphQLId("Manifestation", node.id)} />
        ))}
      </ul>
    </div>
  );
}

type ShelfItemRowProps = {
  id: string;
};

function ShelfItemRow({ id }: Readonly<ShelfItemRowProps>): ReactElement {
  return (
    <li className="border-border bg-surface flex items-center gap-3 rounded-md border p-3">
      <span className="font-mono text-fg-muted text-xs">{id.slice(0, 8)}</span>
    </li>
  );
}

function ShelfDetailSkeleton(): ReactElement {
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
