import { type ReactElement, type ReactNode } from "react";
import { AlertTriangle, Check, Clock, Info, Lock, RefreshCw } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import type { BatchRow } from "@/api/dashboard";
import type { DiscoveryResult } from "@/api/ingestion";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { formatCount, pluralise } from "./format";
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
}: NoticeLineProps): ReactElement {
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

function AcceptedNotice({ result }: { result: DiscoveryResult }): ReactElement {
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
        ? "1 file is waiting and will be picked up automatically. You do not need to scan again."
        : `${formatCount(deferred)} files are waiting and will be picked up automatically. You do not need to scan again.`;
    return <NoticeLine icon={Clock} title="Nothing is ready yet" detail={detail} />;
  }
  return (
    <NoticeLine
      icon={Info}
      title={`Scan started. ${formatCount(queued)} queued, ${formatCount(deferred)} waiting, ${formatCount(suppressed)} unchanged.`}
      detail="Queued files are being imported now. Waiting files are either still being written or waiting for a retry time, and are picked up automatically. Unchanged files were left as they are."
    />
  );
}

function ActivityLine({ batch }: { batch: BatchRow }): ReactElement {
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
}: ScanNoticeProps): ReactElement {
  const failure = !scanPending && scanFailure !== null ? FAILURE_COPY[scanFailure] : null;
  const showActivity = running && latestBatch !== undefined && !scanPending;
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

export { ScanNotice };
