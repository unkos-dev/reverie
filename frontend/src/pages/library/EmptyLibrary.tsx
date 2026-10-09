import { type ReactElement, type ReactNode } from "react";
import { Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { ArrowRight } from "lucide-react";

import { getDashboardActivity } from "@/api/dashboard";
import type { BatchRow } from "@/api/dashboard";
import { getInputCounts } from "@/api/ingestion";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";
import { useAuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";

const SHELL_CLASS =
  "border-border text-fg-muted flex min-h-[40vh] flex-col items-center justify-center rounded-md border border-dashed px-8 py-12 text-center";

const STEPS = [
  {
    title: "Where files go",
    body: "Copy or move EPUB files into the ingestion folder. Subfolders are fine. The folder is set when Reverie is deployed.",
  },
  {
    title: "How it runs",
    body: "Reverie picks up a file once it has stopped changing, checks it, repairs it where it can, and files it. Books appear here as they finish.",
  },
  {
    title: "What to expect",
    body: "Large batches take a while. Files Reverie cannot import stay in the folder, untouched, and are listed on the Ingestion page with the reason.",
  },
] as const;

const COUNT_FORMAT = new Intl.NumberFormat("en-AU");

function plural(n: number, singular: string): string {
  return `${COUNT_FORMAT.format(n)} ${n === 1 ? singular : `${singular}s`}`;
}

function Heading({ children }: { children: ReactNode }): ReactElement {
  return <p className="font-display text-fg mb-2 text-xl font-semibold">{children}</p>;
}

function OpenIngestion({
  primary = false,
  children,
}: {
  primary?: boolean;
  children: ReactNode;
}): ReactElement {
  return (
    <Button asChild variant={primary ? "default" : "outline"} className="mt-6">
      <Link to="/admin/dashboard">
        {children}
        <ArrowRight aria-hidden="true" />
      </Link>
    </Button>
  );
}

function ReaderGuidance(): ReactElement {
  return (
    <div className={SHELL_CLASS}>
      <Heading>No books yet</Heading>
      <p className="max-w-[62ch] text-sm">
        There are no books here yet. Ask the person who manages this library to add some.
      </p>
    </div>
  );
}

function CheckingGuidance(): ReactElement {
  return (
    <div className={SHELL_CLASS} aria-busy="true">
      <span role="status" className="sr-only">
        Checking the ingestion folder
      </span>
      <Skeleton className="h-5 w-52" />
      <Skeleton className="mt-3.5 h-3 w-[26rem] max-w-full" />
      <Skeleton className="mt-2.5 h-3 w-80 max-w-full" />
    </div>
  );
}

function DefaultGuidance(): ReactElement {
  return (
    <div className={SHELL_CLASS}>
      <Heading>No books yet</Heading>
      <p className="max-w-[62ch] text-sm">
        Add EPUB files to the ingestion folder on your server. Reverie watches the folder and files
        each book it can read.
      </p>
      <ol className="mt-6 grid w-full max-w-[68ch] list-none gap-4 text-left">
        {STEPS.map((step, index) => (
          <li key={step.title} className="grid grid-cols-[28px_1fr] gap-3">
            <span
              aria-hidden="true"
              className="border-border-strong text-fg mt-px flex size-6 items-center justify-center rounded-full border font-mono text-xs"
            >
              {index + 1}
            </span>
            <span className="text-sm">
              <strong className="text-fg block font-bold">{step.title}</strong>
              {step.body}
            </span>
          </li>
        ))}
      </ol>
      <OpenIngestion>Open Ingestion</OpenIngestion>
    </div>
  );
}

function UnderwayGuidance({ batch }: { batch: BatchRow }): ReactElement {
  const percent = batch.total === 0 ? 0 : Math.round((batch.completed / batch.total) * 100);
  return (
    <div className={SHELL_CLASS}>
      <Heading>Ingestion is under way</Heading>
      <p className="max-w-[62ch] text-sm">
        {plural(batch.total, "file")} {batch.total === 1 ? "is" : "are"} being checked. Books appear
        here as each one finishes.
      </p>
      <div className="mt-5 grid w-full max-w-[420px] gap-1.5 text-left">
        <span className="text-[13px]">
          {COUNT_FORMAT.format(batch.completed)} imported, {COUNT_FORMAT.format(batch.failed)} did
          not import, {COUNT_FORMAT.format(batch.in_progress)} in progress
        </span>
        <Progress
          value={percent}
          aria-label="Files imported in the latest batch"
          aria-valuetext={`${COUNT_FORMAT.format(batch.completed)} of ${COUNT_FORMAT.format(batch.total)} files imported`}
          className="bg-surface-3 h-1.5 [&>[data-slot=progress-indicator]]:bg-accent-text dark:[&>[data-slot=progress-indicator]]:bg-primary"
        />
      </div>
      <OpenIngestion>Open Ingestion</OpenIngestion>
    </div>
  );
}

function AttentionGuidance({ total }: { total: number }): ReactElement {
  return (
    <div className={SHELL_CLASS}>
      <Heading>No books yet</Heading>
      <p className="max-w-[62ch] text-sm">
        {plural(total, "file")} in the ingestion folder could not be imported. The Ingestion page
        shows why and what to do about each.
      </p>
      <OpenIngestion primary>Review these files</OpenIngestion>
    </div>
  );
}

/**
 * True-empty library. Readers get the holding copy. An admin gets guidance
 * chosen from one read of the ingestion counts and the latest batch: under
 * way, then files needing attention, then the default steps. The reads never
 * poll; they refetch on window focus only, because the admin page's monitor
 * is the only poll owner.
 */
function EmptyLibrary(): ReactElement {
  const { data: me, isLoading: meLoading } = useAuthMe();
  const isAdmin = me?.role === "admin";

  const counts = useQuery({
    queryKey: queryKeys.ingestion.counts(),
    queryFn: ({ signal }) => getInputCounts(signal),
    enabled: isAdmin,
    refetchInterval: false,
  });
  const activity = useQuery({
    queryKey: queryKeys.dashboard.activity(1),
    queryFn: ({ signal }) => getDashboardActivity(1, signal),
    enabled: isAdmin,
    refetchInterval: false,
  });

  if (meLoading) return <CheckingGuidance />;
  if (!isAdmin) return <ReaderGuidance />;
  if (counts.isPending || activity.isPending) return <CheckingGuidance />;

  const batch = activity.data?.batches.at(0);
  if (batch !== undefined && batch.ended_at === null) return <UnderwayGuidance batch={batch} />;
  const attention = counts.data?.attention_total ?? 0;
  if (attention > 0) return <AttentionGuidance total={attention} />;
  return <DefaultGuidance />;
}

export { EmptyLibrary };
