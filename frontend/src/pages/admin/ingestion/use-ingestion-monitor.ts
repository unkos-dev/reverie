/**
 * The one owner of ingestion polling on the admin page.
 *
 * It decides the refetch interval for the activity read, the counts read and
 * page one of every expanded group; those components take the interval as a
 * prop and never start a timer of their own. It also owns the scan mutation
 * and the single notice that reports it, so no component reads or writes
 * those cache keys independently.
 *
 * Polling runs while the latest batch has not ended, and for a grace window
 * after a scan that queued or deferred files (jobs appear only once attempts
 * start, and deferred inputs wait for readiness). Then it stops.
 */
import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { UseQueryResult } from "@tanstack/react-query";

import { ApiError } from "@/api";
import { getDashboardActivity } from "@/api/dashboard";
import type { BatchRow, DashboardActivity } from "@/api/dashboard";
import { getInputCounts, scanIngestion } from "@/api/ingestion";
import type { DiscoveryResult, InputCounts } from "@/api/ingestion";
import { queryKeys } from "@/lib/query/keys";

const POLL_INTERVAL_MS = 3000;
const GRACE_WINDOW_MS = 2 * 60 * 1000;

type ScanFailure = "failed" | "forbidden" | "session";

type IngestionNotice =
  | { kind: "accepted"; result: DiscoveryResult; afterScan: boolean }
  | { kind: "failure"; cause: ScanFailure }
  | { kind: "finished" };

type IngestionMonitor = {
  /** Interval every polled query on the page uses; `false` while idle. */
  pollInterval: number | false;
  counts: UseQueryResult<InputCounts>;
  activity: UseQueryResult<DashboardActivity>;
  latestBatch: BatchRow | undefined;
  running: boolean;
  /** `null` until a scan result or a finished batch has something to say. */
  notice: IngestionNotice | null;
  scanPending: boolean;
  /** A 403 disables the control for the rest of the page session. */
  scanForbidden: boolean;
  scan: () => void;
};

function isRunning(activity: DashboardActivity | undefined): boolean {
  const latest = activity?.batches[0];
  return latest !== undefined && latest.ended_at === null;
}

function changeTotal(counts: InputCounts | undefined): number {
  return (counts?.by_reason ?? [])
    .filter((c) => c.reason === "needs_change" || c.reason === "retries_exhausted")
    .reduce((sum, c) => sum + c.count, 0);
}

function failureCause(err: unknown): ScanFailure {
  if (err instanceof ApiError && err.status === 403) return "forbidden";
  if (err instanceof ApiError && err.status === 401) return "session";
  return "failed";
}

function useIngestionMonitor(options: {
  enabled: boolean;
  activityLimit: number;
}): IngestionMonitor {
  const { enabled, activityLimit } = options;
  const queryClient = useQueryClient();

  const [graceActive, setGraceActive] = useState(false);
  const [graceRuns, setGraceRuns] = useState(0);
  const [notice, setNotice] = useState<IngestionNotice | null>(null);
  const [wasRunning, setWasRunning] = useState(false);
  const countsBeforeScan = useRef(0);

  const activity = useQuery({
    queryKey: queryKeys.dashboard.activity(activityLimit),
    queryFn: ({ signal }) => getDashboardActivity(activityLimit, signal),
    enabled,
    refetchInterval: (query) =>
      isRunning(query.state.data) || graceActive ? POLL_INTERVAL_MS : false,
    refetchOnWindowFocus: false,
  });
  const latestBatch = activity.data?.batches[0];
  const running = isRunning(activity.data);
  const pollInterval = running || graceActive ? POLL_INTERVAL_MS : false;

  const counts = useQuery({
    queryKey: queryKeys.ingestion.counts(),
    queryFn: ({ signal }) => getInputCounts(signal),
    enabled,
    refetchInterval: pollInterval,
    refetchOnWindowFocus: false,
  });

  if (running !== wasRunning) {
    setWasRunning(running);
    if (wasRunning && !running) setNotice({ kind: "finished" });
  }

  useEffect(() => {
    if (graceRuns === 0) return undefined;
    const timer = setTimeout(() => {
      setGraceActive(false);
    }, GRACE_WINDOW_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [graceRuns]);

  const mutation = useMutation({
    mutationFn: scanIngestion,
    onMutate: () => {
      countsBeforeScan.current = changeTotal(
        queryClient.getQueryData(queryKeys.ingestion.counts()),
      );
    },
    onSuccess: (result) => {
      setNotice({ kind: "accepted", result, afterScan: false });
      if (result.queued + result.deferred > 0) {
        setGraceActive(true);
        setGraceRuns((n) => n + 1);
      }
      void queryClient.invalidateQueries({ queryKey: queryKeys.ingestion.counts() }).then(() => {
        const after = changeTotal(queryClient.getQueryData(queryKeys.ingestion.counts()));
        if (after < countsBeforeScan.current) {
          setNotice((current) =>
            current?.kind === "accepted" && current.result === result
              ? { ...current, afterScan: true }
              : current,
          );
        }
      });
      void queryClient.invalidateQueries({ queryKey: queryKeys.ingestion.groupsFirst });
      void queryClient.invalidateQueries({ queryKey: queryKeys.dashboard.activity(activityLimit) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.dashboard.stats() });
    },
    onError: (err) => {
      setNotice({ kind: "failure", cause: failureCause(err) });
    },
  });

  return {
    pollInterval,
    counts,
    activity,
    latestBatch,
    running,
    notice,
    scanPending: mutation.isPending,
    scanForbidden: notice?.kind === "failure" && notice.cause === "forbidden",
    scan: () => {
      mutation.mutate();
    },
  };
}

export { useIngestionMonitor, POLL_INTERVAL_MS, GRACE_WINDOW_MS };
export type { IngestionMonitor, IngestionNotice, ScanFailure };
