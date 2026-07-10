/**
 * Author id to display label resolution, shared by every surface that
 * hydrates an author uuid into a name (the rail's authors section, the
 * masthead filter summary).
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";

import { resolveAuthors } from "@/api";
import { queryKeys } from "@/lib/query/keys";

/** Resolves a set of author ids to display labels, with fallback states for
 *  in-flight and failed lookups. */
export function useAuthorLabels(ids: readonly string[]): { labelFor: (id: string) => string } {
  const { data, isFetching, isError, error } = useQuery({
    queryKey: queryKeys.authors.resolve(ids),
    queryFn: ({ signal }) => resolveAuthors(ids, signal),
    enabled: ids.length > 0,
    staleTime: 60_000,
  });
  // A resolve failure must not read as "author doesn't exist": leave a console
  // breadcrumb (QueryCache.onError forwards only 401s) and label the author as
  // unresolved rather than unknown, mirroring the next-page handler in
  // LibraryContent.
  useEffect(() => {
    if (isError) console.error("[useAuthorLabels] failed to resolve author labels", error);
  }, [isError, error]);
  function labelFor(id: string): string {
    const hit = data?.find((row) => row.id === id);
    if (hit !== undefined) return hit.value;
    if (isFetching) return "resolving…";
    if (isError) return "author unavailable";
    return "Unknown author";
  }
  return { labelFor };
}
