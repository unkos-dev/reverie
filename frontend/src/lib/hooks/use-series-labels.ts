import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";

import { resolveSeries } from "@/api";
import { queryKeys } from "@/lib/query/keys";

export function useSeriesLabels(ids: readonly string[]): { labelFor: (id: string) => string } {
  const { data, isError, error } = useQuery({
    queryKey: queryKeys.series.resolve(ids),
    queryFn: ({ signal }) => resolveSeries(ids, signal),
    enabled: ids.length > 0,
    staleTime: 60_000,
  });
  useEffect(() => {
    if (isError) console.error("[useSeriesLabels] failed to resolve series labels", error);
  }, [isError, error]);
  return {
    labelFor: (id) => data?.find((row) => row.id === id)?.value ?? `Series ${id.slice(0, 8)}…`,
  };
}
