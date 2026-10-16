import { type ReactElement, type ReactNode } from "react";
import { AlertTriangle, Check, Clock, Info, Lock, RefreshCw } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Link } from "react-router";
import type { UseQueryResult } from "@tanstack/react-query";

import { ApiError } from "@/api";
import type { BatchRow } from "@/api/dashboard";
import type { DiscoveryResult, IngestionStatus } from "@/api/ingestion";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { formatDateTime } from "@/lib/format/datetime";

import { formatCount, pluralise } from "./format";
import { DEPENDENCY_COPY } from "./reason-copy";
import type { IngestionNotice, ScanFailure } from "./use-ingestion-monitor";

type NoticeLineProps = {
  icon: LucideIcon;
  title: string;
  detail: string;
  boxed?: boolean;
  spin?: boolean;
  extra?: ReactNode;
};

function NoticeLine({
  icon: Icon,
  title,
  detail,
  boxed = false,
  spin = false,
  extra,
}: Readonly<NoticeLineProps>): ReactElement {
  return (
    <div
      className={cn(
        "flex items-start gap-3 text-sm",
        boxed && "border-border-strong rounded-xl border px-4 py-3",
      )}
    >
      <Icon
        aria-hidden="true"
        className={cn("text-fg-muted mt-[3px] size-4 shrink-0", spin && "motion-safe:animate-spin")}
      />
      <div className="flex max-w-[100ch] flex-col gap-0.5">
        <strong className="font-bold">{title}</strong>
        <span className="text-fg-muted text-sm">{detail}</span>
        {extra}
      </div>
    </div>
  );
}

function batchSummary(batch: BatchRow, includeInProgress: boolean): string {
  const parts = [
    `${formatCount(batch.completed)} imported`,
    `${formatCount(batch.failed)} did not import`,
  ];
  if (batch.skipped > 0) parts.push(`${formatCount(batch.skipped)} skipped`);
  if (includeInProgress) parts.push(`${formatCount(batch.in_progress)} in progress`);
  return `${pluralise(batch.total, "file")} in the latest batch: ${parts.join(", ")}`;
}

function AcceptedNotice({ result }: Readonly<{ result: DiscoveryResult }>): ReactElement {
  const { queued, deferred, suppressed } = result;
  if (queued === 0 && deferred === 0 && suppressed === 0) {
    return (
      <NoticeLine
        icon={Check}
        title="Nothing to import"
        detail="The folder has no new or changed files."
      />
    );
  }
  if (queued === 0 && deferred === 0) {
    const detail =
      suppressed === 1
        ? "1 file was left as it is, because nothing about it has changed."
        : `${formatCount(suppressed)} files were left as they are, because nothing about them has changed.`;
    return <NoticeLine icon={Check} title="Nothing new to import" detail={detail} />;
  }
  if (queued === 0) {
    const detail =
      deferred === 1
        ? "1 file is waiting for readiness or a retry time. You do not need to scan again."
        : `${formatCount(deferred)} files are waiting for readiness or a retry time. You do not need to scan again.`;
    return <NoticeLine icon={Clock} title="Nothing is ready yet" detail={detail} />;
  }
  return (
    <NoticeLine
      icon={Info}
      title={`Scan started. ${formatCount(queued)} queued, ${formatCount(deferred)} waiting, ${formatCount(suppressed)} unchanged.`}
      detail="The scan queued new or changed files. Waiting files are either still being written or waiting for a retry time. Unchanged files were left as they are."
    />
  );
}

function ActivityLine({ batch }: Readonly<{ batch: BatchRow }>): ReactElement {
  return (
    <span className="ml-7 text-[13px]">
      <b className="font-bold">Latest activity.</b>{" "}
      <span className="text-fg-muted">
        {batchSummary(batch, true)}. Updating every few seconds.
      </span>
    </span>
  );
}

const FAILURE_COPY: Record<ScanFailure, { icon: LucideIcon; title: string; detail: string }> = {
  failed: {
    icon: AlertTriangle,
    title: "The scan did not start",
    detail:
      "Reverie could not complete the request, so nothing was changed. Try again. If it keeps happening, check the server log.",
  },
  forbidden: {
    icon: Lock,
    title: "This account cannot start a scan",
    detail: "Sign in as an administrator to scan the ingestion folder.",
  },
  session: {
    icon: Lock,
    title: "Your session has ended",
    detail: "Sign in again to scan the ingestion folder.",
  },
};

type ScanNoticeProps = {
  notice: IngestionNotice | null;
  failure: ScanFailure | null;
  scanPending: boolean;
  running: boolean;
  latestBatch: BatchRow | undefined;
  onRetry: () => void;
  blocked: boolean;
};

/**
 * Scan and activity feedback. The polite status region is always mounted so
 * screen readers pick up the first message; failures sit beside it as alerts.
 */
function ScanNotice({
  notice,
  failure: scanFailure,
  scanPending,
  running,
  latestBatch,
  onRetry,
  blocked,
}: Readonly<ScanNoticeProps>): ReactElement {
  const failure = !scanPending && scanFailure !== null ? FAILURE_COPY[scanFailure] : null;
  const showActivity = running && latestBatch !== undefined && !scanPending && !blocked;
  let body: ReactNode = null;
  if (scanPending) {
    body = (
      <NoticeLine
        icon={RefreshCw}
        spin
        title="Scanning the ingestion folder"
        detail="Looking at every file. This usually takes a few seconds."
      />
    );
  } else if (notice?.kind === "accepted") {
    body = <AcceptedNotice result={notice.result} />;
  } else if (notice?.kind === "finished" && latestBatch !== undefined) {
    body = (
      <NoticeLine
        icon={Check}
        title="Latest activity has finished"
        detail={`${batchSummary(latestBatch, false)}. Batches are not tied to a particular scan. Anything that did not import is under Needs attention.`}
      />
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <div role="status" aria-live="polite" className="flex flex-col gap-2">
        {body}
        {showActivity ? <ActivityLine batch={latestBatch} /> : null}
        {!scanPending && notice?.kind === "accepted" && notice.afterScan ? (
          <NoticeLine
            icon={RefreshCw}
            title="Files were sent for another try"
            detail="Files marked Needs a change are back in the queue and return here if they fail again. Rejected files were left as they are, because they have not changed."
          />
        ) : null}
      </div>
      {failure ? (
        <div role="alert">
          <NoticeLine
            boxed
            icon={failure.icon}
            title={failure.title}
            detail={failure.detail}
            extra={
              scanFailure === "failed" ? (
                <span className="mt-2">
                  <Button type="button" variant="outline" size="sm" onClick={onRetry}>
                    Try again
                  </Button>
                </span>
              ) : undefined
            }
          />
        </div>
      ) : null}
    </div>
  );
}

function PipelineNotice({
  status,
}: Readonly<{ status: UseQueryResult<IngestionStatus> }>): ReactElement {
  let body: ReactNode = null;
  let failure: ReactNode = null;
  if (status.isError) {
    const code = status.error instanceof ApiError ? status.error.status : null;
    if (code === 401 || code === 403) {
      failure = (
        <div role="alert">
          <NoticeLine
            boxed
            icon={Lock}
            title={
              code === 401 ? "Your session has ended" : "This account cannot see ingestion status"
            }
            detail={
              code === 401
                ? "Sign in again to see ingestion status."
                : "Sign in as an administrator."
            }
          />
        </div>
      );
    } else {
      body = (
        <NoticeLine
          icon={Info}
          title="Could not check whether ingestion is running."
          detail="Files may still be importing."
          extra={
            <span className="mt-2">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => {
                  void status.refetch();
                }}
              >
                Try again
              </Button>
            </span>
          }
        />
      );
    }
  } else if (status.data !== undefined && status.data.blockers.length > 0) {
    const administrator = status.data.blockers.some(
      (blocker) => blocker.kind === "paused_by_admin",
    );
    const dependency = status.data.blockers.find((blocker) => blocker.kind === "dependency");
    const restarting = status.data.blockers.some((blocker) => blocker.kind === "restarting");
    const dependencyCopy = dependency === undefined ? null : DEPENDENCY_COPY[dependency.reason];
    body = (
      <section
        aria-labelledby="ingestion-blockers-title"
        className="border-border-strong rounded-xl border px-4 py-3 text-sm"
      >
        <h2 id="ingestion-blockers-title" className="font-bold">
          Imports are paused
        </h2>
        <div className="mt-3 space-y-3 text-fg-muted">
          {administrator ? (
            <div>
              <p>New imports have been paused by an administrator.</p>
              <p>
                <strong className="font-bold text-fg">What to check.</strong> Turn on Import files
                in Acquisition settings when you are ready to resume.
              </p>
              <Link
                className="font-medium text-fg underline underline-offset-4"
                to="/admin/settings/acquisition"
              >
                Open Acquisition settings
              </Link>
            </div>
          ) : null}
          {dependency !== undefined && dependencyCopy !== null ? (
            <div>
              <p>{dependencyCopy.reason}</p>
              <p>
                <strong className="font-bold text-fg">What to check.</strong> {dependencyCopy.check}
              </p>
              <p>
                Dependency problem since{" "}
                <time dateTime={dependency.since}>{formatDateTime(dependency.since)}</time>.
              </p>
            </div>
          ) : null}
          {restarting ? (
            <div>
              <p>The ingestion worker is restarting.</p>
              <p>
                <strong className="font-bold text-fg">What to check.</strong> Wait for Reverie to
                restart the worker. If it keeps restarting, check the server log.
              </p>
            </div>
          ) : null}
          <p>
            Queued files are kept. Work already started can finish, and scans and recovery continue.
            New imports resume when Import files is on, dependencies are available and the worker is
            running.
          </p>
        </div>
      </section>
    );
  }
  return (
    <div className={body === null && failure === null ? "sr-only" : "space-y-2"}>
      <div role="status" aria-label="Ingestion status" aria-live="polite">
        {body}
      </div>
      {failure}
    </div>
  );
}

export { PipelineNotice, ScanNotice };
