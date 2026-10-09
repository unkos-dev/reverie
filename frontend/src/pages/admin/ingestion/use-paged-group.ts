/**
 * Rows of one reason group: page one is a polled query, later pages are
 * opened by "Show more" and never refetched on a timer or on focus.
 *
 * Splitting the pages is what keeps polling cheap. An infinite query would
 * refetch every loaded page on each tick, so a deep scroll would multiply the
 * cost of keeping the group current.
 */
import { useState } from "react";
import { useQueries, useQuery } from "@tanstack/react-query";

type Page<TItem> = { items: TItem[]; next_cursor?: string | null | undefined };

type PagedGroupOptions<TItem> = {
  firstKey: readonly unknown[];
  afterKey: (cursor: string) => readonly unknown[];
  fetchPage: (cursor: string | undefined, signal: AbortSignal) => Promise<Page<TItem>>;
  getId: (item: TItem) => string;
  /** Interval for page one only; `false` for no timer. */
  refetchInterval: number | false;
  refetchOnWindowFocus: boolean;
};

type PagedGroup<TItem> = {
  rows: TItem[];
  isPending: boolean;
  isError: boolean;
  hasMore: boolean;
  isLoadingMore: boolean;
  isMoreError: boolean;
  /** Rows added by the most recent "Show more", for the polite announcement. */
  loadedMore: number;
  showMore: () => void;
  retry: () => void;
};

function usePagedGroup<TItem>(options: PagedGroupOptions<TItem>): PagedGroup<TItem> {
  const { firstKey, afterKey, fetchPage, getId, refetchInterval, refetchOnWindowFocus } = options;
  const [cursors, setCursors] = useState<string[]>([]);

  const first = useQuery({
    queryKey: firstKey,
    queryFn: ({ signal }) => fetchPage(undefined, signal),
    refetchInterval,
    refetchOnWindowFocus,
  });

  const later = useQueries({
    queries: cursors.map((cursor) => ({
      queryKey: afterKey(cursor),
      queryFn: ({ signal }: { signal: AbortSignal }) => fetchPage(cursor, signal),
      staleTime: Infinity,
      refetchOnWindowFocus: false,
    })),
  });

  const seen = new Set<string>();
  const rows: TItem[] = [];
  const pages = [first.data, ...later.map((q) => q.data)];
  for (const page of pages) {
    for (const item of page?.items ?? []) {
      const id = getId(item);
      if (seen.has(id)) continue;
      seen.add(id);
      rows.push(item);
    }
  }

  const lastLater = later.at(-1);
  const tail = lastLater === undefined ? first.data : lastLater.data;
  const tailCursor = tail?.next_cursor ?? null;
  const loadedMore = lastLater?.data?.items.length ?? 0;

  function showMore(): void {
    if (tailCursor === null || lastLater?.isFetching === true) return;
    setCursors((prev) => [...prev, tailCursor]);
  }

  function retry(): void {
    if (lastLater?.isError === true) void lastLater.refetch();
    else void first.refetch();
  }

  return {
    rows,
    isPending: first.isPending,
    isError: first.isError && first.data === undefined,
    hasMore: tailCursor !== null,
    isLoadingMore: lastLater?.isFetching === true,
    isMoreError: lastLater?.isError === true,
    loadedMore,
    showMore,
    retry,
  };
}

export { usePagedGroup };
export type { PagedGroup };
