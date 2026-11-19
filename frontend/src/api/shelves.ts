/**
 * Caller-owned shelf reads use Relay's shared connection and complete its cursor pages.
 *
 * Read failure has three owners:
 * - Primary pages, `pages/shelves/ShelvesListPage` and `pages/shelves/ShelfDetailPage`,
 *   surface failure through their owning route boundary.
 * - Loaders, `routes/shelves` and `routes/shelf-detail`, prepare and dispose native
 *   query references for those pages. Failed preparation reaches the owning route.
 * - Auxiliary consumers, `components/shell/FilterRail`, `pages/library/FilterChips`
 *   and `pages/library/LibraryPage`, stay mounted and expose their existing degraded
 *   state. The shared read logs failures, including failed continuation pages.
 *
 * Creating, renaming and deleting shelves belongs to the owning page. A failed
 * create acknowledgement triggers a read, never an automatic create replay.
 * Membership writes retain the Library's serial per-book result owner.
 */
import { useEffect, useState, useTransition } from "react";
import { fetchQuery, usePaginationFragment, useRelayEnvironment } from "react-relay";
import { createOperationDescriptor, getRequest } from "relay-runtime";
import { readerShelves, readerShelvesQuery } from "@/lib/relay/reader";
import type { readerShelves$key } from "@/lib/relay/__generated__/readerShelves.graphql";
import type { readerShelvesQuery as ShelvesQuery } from "@/lib/relay/__generated__/readerShelvesQuery.graphql";
import type { readerShelvesPaginationQuery as ShelvesPagination } from "@/lib/relay/__generated__/readerShelvesPaginationQuery.graphql";
export type Shelf = {
  id: string;
  name: string;
  is_system: boolean;
  created_at: string;
  updated_at: string;
  item_count: number;
};

export function useShelves(
  initial?: readerShelves$key,
  enabled = true,
): {
  data: Shelf[];
  isLoading: boolean;
  isError: boolean;
  error: Error | null;
  refetch: () => void;
} {
  const environment = useRelayEnvironment();
  const [reference, setReference] = useState<readerShelves$key | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [, startTransition] = useTransition();
  useEffect(() => {
    if (!enabled || initial !== undefined) return;
    const retention = environment.retain(
      createOperationDescriptor(getRequest(readerShelvesQuery), {}),
    );
    const subscription = fetchQuery<ShelvesQuery>(
      environment,
      readerShelvesQuery,
      {},
      { fetchPolicy: "store-or-network" },
    ).subscribe({
      next: setReference,
      error: (failure: Error) => {
        setError(failure);
        console.error("[useShelves] shelves fetch failed", failure);
      },
    });
    return () => {
      subscription.unsubscribe();
      retention.dispose();
    };
  }, [environment, initial, enabled]);
  const { data, hasNext, isLoadingNext, loadNext, refetch } = usePaginationFragment<
    ShelvesPagination,
    readerShelves$key
  >(readerShelves, initial ?? reference);
  useEffect(() => {
    if (error === null && hasNext && !isLoadingNext)
      loadNext(50, {
        onComplete: (failure) => {
          if (failure !== null) {
            setError(failure);
            console.error("[useShelves] shelf page failed", failure);
          }
        },
      });
  }, [hasNext, isLoadingNext, loadNext, error]);
  return {
    data:
      data?.shelves.edges.map(({ node }) => ({
        id: node.id,
        name: node.name,
        is_system: node.isSystem,
        item_count: node.itemCount,
        created_at: node.createdAt,
        updated_at: node.updatedAt,
      })) ?? [],
    isLoading: error === null && (data == null || hasNext || isLoadingNext),
    isError: error !== null,
    error,
    refetch: () => {
      startTransition(() => {
        refetch({}, { fetchPolicy: "network-only", onComplete: setError });
      });
    },
  };
}
