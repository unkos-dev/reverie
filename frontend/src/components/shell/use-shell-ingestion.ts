import { useQuery } from "@tanstack/react-query";
import { matchPath, useLocation } from "react-router";

import { getDashboardActivity, getDashboardStats } from "@/api/dashboard";
import { getInputCounts } from "@/api/ingestion";
import { useAuthMe } from "@/hooks/useAuthMe";
import { ACTIVITY_LIMIT } from "@/lib/ingestion";
import { queryKeys } from "@/lib/query/keys";

export function useShellIngestion(): {
  attention: number;
  scanRunning: boolean;
  enrichmentRunning: boolean;
} {
  const { pathname } = useLocation();
  const isIngestionRoute = matchPath("/admin/dashboard", pathname) !== null;
  const { data: me } = useAuthMe();
  const enabled = me?.role === "admin";
  const activity = useQuery({
    queryKey: queryKeys.dashboard.activity(ACTIVITY_LIMIT),
    queryFn: ({ signal }) => getDashboardActivity(ACTIVITY_LIMIT, signal),
    enabled,
    refetchInterval: (query) => {
      if (isIngestionRoute) return false;
      return query.state.data?.batches[0]?.ended_at === null ? 3000 : 30_000;
    },
    refetchOnWindowFocus: !isIngestionRoute,
  });
  const scanRunning = activity.data?.batches[0]?.ended_at === null;
  const stats = useQuery({
    queryKey: queryKeys.dashboard.stats(),
    queryFn: ({ signal }) => getDashboardStats(signal),
    enabled,
    refetchInterval: (query) =>
      scanRunning ||
      query.state.data?.enrichment_breakdown.some(
        (entry) => entry.status === "in_progress" && entry.count > 0,
      )
        ? 3000
        : 30_000,
    refetchOnWindowFocus: true,
  });
  const enrichmentRunning =
    stats.data?.enrichment_breakdown.some(
      (entry) => entry.status === "in_progress" && entry.count > 0,
    ) ?? false;
  const counts = useQuery({
    queryKey: queryKeys.ingestion.counts(),
    queryFn: ({ signal }) => getInputCounts(signal),
    enabled,
    refetchInterval: !isIngestionRoute && (scanRunning || enrichmentRunning) ? 3000 : false,
    refetchOnWindowFocus: !isIngestionRoute,
  });
  const attention = counts.data?.attention_total ?? 0;
  return { attention, scanRunning, enrichmentRunning };
}
