/**
 * `/admin/dashboard` — admin-only library-health dashboard.
 *
 * Renders six metric groups (top-line stats, validation breakdown,
 * enrichment breakdown, metadata coverage, storage by format, recent
 * ingestion batches) from the two `/api/dashboard/*` endpoints using only
 * existing shadcn/ui primitives — no chart library. Non-admin callers are
 * redirected to `/library`.
 */
import { type ReactElement } from "react";
import { Navigate } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { Activity, AlertTriangle, BookOpen, HardDrive } from "lucide-react";

import { useAuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";
import { getDashboardStats, getDashboardActivity } from "@/api/dashboard";
import type { DashboardStats, DashboardActivity } from "@/api/dashboard";
import { ApiError } from "@/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";

/** Recent-batch page size requested from `/api/dashboard/activity`. */
export const ACTIVITY_LIMIT = 20;

const NUMBER_FORMAT = new Intl.NumberFormat("en-US");

/** Format an integer with thousands separators (`1204` → `"1,204"`). */
function formatNumber(n: number): string {
  return NUMBER_FORMAT.format(n);
}

/** Human-readable byte size (`3650000000` → `"3.4 GB"`). */
function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, exp);
  return `${value.toFixed(exp === 0 ? 0 : 1)} ${units[exp]}`;
}

/** Coverage percentage, guarding divide-by-zero on an empty library. */
function pct(numerator: number, total: number): number {
  return total === 0 ? 0 : Math.round((numerator / total) * 100);
}

function countFor(breakdown: DashboardStats["validation_breakdown"], status: string): number {
  return breakdown.find((b) => b.status === status)?.count ?? 0;
}

function DashboardPage(): ReactElement {
  const { data: me, isLoading: meLoading, isError: meError } = useAuthMe();
  const isAdmin = me?.role === "admin";

  const {
    data: stats,
    isLoading: statsLoading,
    error: statsError,
  } = useQuery({
    queryKey: queryKeys.dashboard.stats(),
    queryFn: ({ signal }) => getDashboardStats(signal),
    enabled: isAdmin,
  });

  const { data: activity } = useQuery({
    queryKey: queryKeys.dashboard.activity(ACTIVITY_LIMIT),
    queryFn: ({ signal }) => getDashboardActivity(ACTIVITY_LIMIT, signal),
    enabled: isAdmin,
  });

  if (meLoading) {
    return (
      <div className="mx-auto max-w-6xl p-6">
        <Skeleton className="h-8 w-48" />
      </div>
    );
  }

  if (meError) {
    return (
      <div className="mx-auto max-w-6xl p-6">
        <p className="text-destructive">
          Could not verify your identity. Please refresh the page or try again later.
        </p>
      </div>
    );
  }

  if (!isAdmin) {
    return <Navigate to="/library" replace />;
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6 p-6">
      <h1 className="text-2xl font-bold text-fg">Library health</h1>

      {statsLoading && (
        <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
          {Array.from({ length: 4 }, (_, i) => (
            <Skeleton key={`stat-skel-${String(i)}`} className="h-24 w-full" />
          ))}
        </div>
      )}

      {statsError && (
        <p className="text-destructive">
          Failed to load metrics:{" "}
          {statsError instanceof ApiError ? statsError.detail : statsError.message}
        </p>
      )}

      {stats && <StatsView stats={stats} />}

      <RecentBatches activity={activity?.batches ?? []} />
    </div>
  );
}

interface StatCardProps {
  label: string;
  value: string;
  icon: ReactElement;
}

function StatCard({ label, value, icon }: StatCardProps): ReactElement {
  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
        <CardTitle className="text-sm font-medium text-fg-muted">{label}</CardTitle>
        {icon}
      </CardHeader>
      <CardContent>
        <div className="text-2xl font-bold text-fg">{value}</div>
      </CardContent>
    </Card>
  );
}

function StatsView({ stats }: { stats: DashboardStats }): ReactElement {
  const failedValidation = countFor(stats.validation_breakdown, "degraded");
  const cov = stats.metadata_coverage;
  const coverageRows = [
    { label: "Description", n: cov.has_description },
    { label: "Language", n: cov.has_language },
    { label: "ISBN-13", n: cov.has_isbn_13 },
    { label: "Cover", n: cov.has_cover },
  ];

  return (
    <>
      <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
        <StatCard
          label="Books"
          value={formatNumber(stats.total_manifestations)}
          icon={<BookOpen className="size-4 text-fg-muted" />}
        />
        <StatCard
          label="Works"
          value={formatNumber(stats.total_works)}
          icon={<BookOpen className="size-4 text-fg-muted" />}
        />
        <StatCard
          label="Storage"
          value={formatBytes(stats.storage_total_bytes)}
          icon={<HardDrive className="size-4 text-fg-muted" />}
        />
        <StatCard
          label="Degraded"
          value={formatNumber(failedValidation)}
          icon={<AlertTriangle className="size-4 text-fg-muted" />}
        />
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Validation</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            {stats.validation_breakdown.map((b) => (
              <div key={b.status} className="flex items-center justify-between">
                <Badge variant="outline">{b.status}</Badge>
                <span className="text-sm font-medium text-fg">{formatNumber(b.count)}</span>
              </div>
            ))}
            <p className="pt-2 text-xs text-fg-muted">
              {formatNumber(stats.clean_non_epub_count)} of the clean files are non-EPUB formats not
              structurally validated (UNK-313).
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Enrichment</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            {stats.enrichment_breakdown.map((b) => (
              <div key={b.status} className="flex items-center justify-between">
                <Badge variant="outline">{b.status}</Badge>
                <span className="text-sm font-medium text-fg">{formatNumber(b.count)}</span>
              </div>
            ))}
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Metadata coverage</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            {coverageRows.map((row) => {
              const percent = pct(row.n, cov.total);
              return (
                <div key={row.label} className="space-y-1">
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-fg-muted">{row.label}</span>
                    <span className="font-medium text-fg">{percent}%</span>
                  </div>
                  <Progress value={percent} />
                </div>
              );
            })}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Storage by format</CardTitle>
          </CardHeader>
          <CardContent>
            {stats.storage_by_format.length === 0 ? (
              <p className="text-sm text-fg-muted">No files yet.</p>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Format</TableHead>
                    <TableHead className="text-right">Files</TableHead>
                    <TableHead className="text-right">Size</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {stats.storage_by_format.map((f) => (
                    <TableRow key={f.format}>
                      <TableCell className="font-medium uppercase">{f.format}</TableCell>
                      <TableCell className="text-right">{formatNumber(f.count)}</TableCell>
                      <TableCell className="text-right">{formatBytes(f.bytes)}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      </div>
    </>
  );
}

function RecentBatches({ activity }: { activity: DashboardActivity["batches"] }): ReactElement {
  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0">
        <CardTitle>Recent ingestion batches</CardTitle>
        <Activity className="size-4 text-fg-muted" />
      </CardHeader>
      <CardContent>
        {activity.length === 0 ? (
          <p className="text-sm text-fg-muted">No ingestion runs recorded yet.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Started</TableHead>
                <TableHead className="text-right">Total</TableHead>
                <TableHead className="text-right">Done</TableHead>
                <TableHead className="text-right">Failed</TableHead>
                <TableHead className="text-right">Skipped</TableHead>
                <TableHead className="text-right">In&nbsp;progress</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {activity.map((b) => (
                <TableRow key={b.batch_id}>
                  <TableCell className="font-medium">
                    {new Date(b.started_at).toLocaleString()}
                    {b.ended_at === null && (
                      <Badge variant="secondary" className="ml-2">
                        running
                      </Badge>
                    )}
                  </TableCell>
                  <TableCell className="text-right">{formatNumber(b.total)}</TableCell>
                  <TableCell className="text-right">{formatNumber(b.completed)}</TableCell>
                  <TableCell className="text-right">{formatNumber(b.failed)}</TableCell>
                  <TableCell className="text-right">{formatNumber(b.skipped)}</TableCell>
                  <TableCell className="text-right">{formatNumber(b.in_progress)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

export { DashboardPage };
