/**
 * Production `/shelves/:id` page.
 *
 * Renders the shelf's items in the order the server returns them: when
 * each was added.
 */
import { useSuspenseQuery } from "@tanstack/react-query";
import { Suspense, type ReactElement } from "react";
import { useParams } from "react-router";

import { getShelf, type ShelfWithItems } from "@/api";
import { Skeleton } from "@/components/ui/skeleton";
import { queryKeys } from "@/lib/query/keys";

/** Top-level page component (Suspense boundary + content). */
export function ShelfDetailPage(): ReactElement {
  return (
    <Suspense fallback={<ShelfDetailSkeleton />}>
      <ShelfDetailContent />
    </Suspense>
  );
}

function ShelfDetailContent(): ReactElement {
  const params = useParams<{ id: string }>();
  const id = params.id ?? "";
  const { data } = useSuspenseQuery<ShelfWithItems>({
    queryKey: queryKeys.shelves.detail(id),
    queryFn: ({ signal }) => getShelf(id, signal),
  });

  return (
    <div className="mx-auto max-w-3xl px-6 py-10 sm:px-10">
      <header className="mb-6">
        <p className="text-fg-muted text-xs uppercase tracking-wider">Shelf</p>
        <h1 className="font-display mt-1 text-3xl font-semibold tracking-tight text-fg">
          {data.name}
        </h1>
        <p className="text-fg-muted mt-2 text-sm">
          {data.items.length} {data.items.length === 1 ? "item" : "items"}
        </p>
      </header>
      <ul className="space-y-2">
        {data.items.map((item) => (
          <ShelfItemRow key={item.manifestation_id} id={item.manifestation_id} />
        ))}
      </ul>
    </div>
  );
}

type ShelfItemRowProps = {
  id: string;
};

function ShelfItemRow({ id }: ShelfItemRowProps): ReactElement {
  return (
    <li className="border-border bg-surface-1 flex items-center gap-3 rounded-md border p-3">
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
