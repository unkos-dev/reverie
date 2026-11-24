/**
 * Production `/shelves/:id` page.
 *
 * Renders the shelf's items in the order the server returns them: when
 * each was added.
 */
import {
  graphql,
  useFragment,
  usePaginationFragment,
  usePreloadedQuery,
  useQueryLoader,
  type PreloadedQuery,
} from "react-relay";
import { decodeGraphQLId } from "@/api/graphql";
import type { ShelfDetailPageQuery as ShelfQuery } from "@/lib/relay/__generated__/ShelfDetailPageQuery.graphql";
import type { ShelfDetailPage_shelf$key } from "@/lib/relay/__generated__/ShelfDetailPage_shelf.graphql";
import type { ShelfDetailPage_items$key } from "@/lib/relay/__generated__/ShelfDetailPage_items.graphql";
import type { ShelfDetailPageItemsPaginationQuery as ItemsPagination } from "@/lib/relay/__generated__/ShelfDetailPageItemsPaginationQuery.graphql";
import { useEffect, useState, Suspense, type ReactElement } from "react";
import { useLoaderData } from "react-router";

import { NotFound } from "@/components/RootErrorBoundary";
import { Skeleton } from "@/components/ui/skeleton";

import type { loader } from "@/routes/shelf-detail";

const shelfDetailPageQuery = graphql`
  query ShelfDetailPageQuery($id: ID!) @throwOnFieldError {
    shelf(id: $id) {
      ...ShelfDetailPage_shelf
    }
  }
`;

const shelfFragment = graphql`
  fragment ShelfDetailPage_shelf on Shelf @throwOnFieldError {
    name
    itemCount
    ...ShelfDetailPage_items
  }
`;

const shelfItems = graphql`
  fragment ShelfDetailPage_items on Shelf
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 50 }, cursor: { type: "String" })
  @refetchable(queryName: "ShelfDetailPageItemsPaginationQuery", preferFetchable: true) {
    items(first: $count, after: $cursor) @connection(key: "ShelfDetailPage_items") {
      edges {
        node {
          id
        }
      }
    }
  }
`;

/** Top-level page component (Suspense boundary + content). */
export function ShelfDetailPage(): ReactElement | null {
  const prepared = useLoaderData<typeof loader>();
  const [reference] = useQueryLoader<ShelfQuery>(shelfDetailPageQuery, prepared);
  if (reference == null || prepared === null) return null;
  return (
    <Suspense fallback={<ShelfDetailSkeleton />}>
      <ShelfDetailContent reference={reference} />
    </Suspense>
  );
}

function ShelfDetailContent({
  reference,
}: Readonly<{
  reference: PreloadedQuery<ShelfQuery>;
}>): ReactElement {
  const query = usePreloadedQuery<ShelfQuery>(shelfDetailPageQuery, reference);
  const data = useFragment<ShelfDetailPage_shelf$key>(shelfFragment, query.shelf);
  const {
    data: items,
    hasNext,
    isLoadingNext,
    loadNext,
  } = usePaginationFragment<ItemsPagination, ShelfDetailPage_items$key>(shelfItems, data);
  const [error, setError] = useState<Error | null>(null);
  useEffect(() => {
    if (hasNext && !isLoadingNext) loadNext(50, { onComplete: setError });
  }, [hasNext, isLoadingNext, loadNext]);
  if (error !== null) throw error;
  if (data == null) return <NotFound />;
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
          <ShelfItemRow key={node.id} id={decodeGraphQLId(node.id)} />
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
