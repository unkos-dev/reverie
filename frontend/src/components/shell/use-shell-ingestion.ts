import { useQuery } from "@tanstack/react-query";

import { getDashboardActivity, getDashboardStats } from "@/api/dashboard";
import { getInputCounts } from "@/api/ingestion";
import { useAuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";

export function useShellIngestion(): {
  attention: number;
  scanRunning: boolean;
  enrichmentRunning: boolean;
} {
  const { data: me } = useAuthMe();
  const enabled = me?.role === "admin";
  const activity = useQuery({
    queryKey: queryKeys.dashboard.activity(1),
    queryFn: ({ signal }) => getDashboardActivity(1, signal),
    enabled,
    refetchInterval: (query) => (query.state.data?.batches[0]?.ended_at === null ? 3000 : false),
    refetchOnWindowFocus: true,
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
        : false,
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
    refetchInterval: scanRunning || enrichmentRunning ? 3000 : false,
    refetchOnWindowFocus: true,
  });
  const attention = counts.data?.attention_total ?? 0;
  return { attention, scanRunning, enrichmentRunning };
}
