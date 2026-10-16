import { type ReactElement, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";

import { formatCount } from "./format";
import type { InputStatusTag } from "./reason-copy";

type GroupShellProps = {
  headerControl?: ReactNode;
  feedback?: ReactNode;
  title: string;
  action: string;
  tag?: InputStatusTag;
  /** Already formatted, for example `"2 files"`. */
  countLabel: string;
  muted?: boolean;
  defaultOpen: boolean;
  /** Mounted only while the group is open, so a closed group makes no requests. */
  children: ReactNode;
};

function StatusTag({ tag }: Readonly<{ tag: InputStatusTag }>): ReactElement {
  return (
    <Badge
      variant="outline"
      className={cn(
        "border-border-strong text-fg-muted rounded-sm px-1.5 font-mono text-[11px] font-normal tracking-wider uppercase",
        tag === "Rejected" && "border-fg-muted text-fg font-bold",
      )}
    >
      {tag}
    </Badge>
  );
}

function GroupShell({
  headerControl,
  feedback,
  title,
  action,
  tag,
  countLabel,
  muted = false,
  defaultOpen,
  children,
}: Readonly<GroupShellProps>): ReactElement {
  return (
    <Collapsible defaultOpen={defaultOpen} className="border-border border-t">
      <div className="flex flex-wrap items-center gap-x-3 pr-4">
        <CollapsibleTrigger asChild>
          <button
            type="button"
            className="group/trigger focus-visible:-outline-offset-2! hover:bg-surface-2/60 grid min-w-0 flex-1 basis-full grid-cols-[20px_minmax(0,1fr)] sm:basis-auto sm:grid-cols-[20px_minmax(0,1fr)_auto] items-start gap-3 px-4 py-3.5 text-left"
          >
            <ChevronDown
              aria-hidden="true"
              className="text-fg-muted mt-1 size-4 transition-transform motion-reduce:transition-none group-data-[state=closed]/trigger:-rotate-90"
            />
            <span className="flex flex-col gap-0.5">
              <strong
                className={cn("text-[15px]", muted ? "text-fg-muted font-medium" : "font-bold")}
              >
                {title}
              </strong>
              <span className="text-fg-muted text-[13px]">{action}</span>
            </span>
            <span className="col-start-2 flex flex-wrap items-center gap-2 sm:col-start-auto">
              {tag ? <StatusTag tag={tag} /> : null}
              <span className="font-mono text-[13px]">{countLabel}</span>
            </span>
          </button>
        </CollapsibleTrigger>
        {headerControl}
      </div>
      {feedback}
      <CollapsibleContent className="border-border bg-canvas-2/55 border-t">
        {children}
      </CollapsibleContent>
    </Collapsible>
  );
}

export { GroupShell };

export type BulkRetryState =
  | { kind: "pending"; count: number }
  | { kind: "success"; queued: number; remaining: number; countsVersion: number }
  | { kind: "failure"; cause: "failed" | "forbidden" | "session" };

export function BulkRetryButton({
  state,
  count,
  countsVersion,
  noun,
  onRetry,
}: Readonly<{
  state: BulkRetryState | undefined;
  count: number;
  countsVersion: number;
  noun: "file" | "book";
  onRetry: (count: number) => void;
}>): ReactElement {
  const fresh = state?.kind === "success" && countsVersion > state.countsVersion;
  const remaining = state?.kind === "success" && !fresh ? state.remaining : count;
  const disabled =
    state?.kind === "pending" ||
    (state?.kind === "failure" && state.cause === "forbidden") ||
    (state?.kind === "success" && remaining === 0);
  let label = count === 1 ? `Retry 1 ${noun}` : `Retry all ${formatCount(count)}`;
  if (state?.kind === "pending") label = `Queuing ${formatCount(state.count)}`;
  if (state?.kind === "failure") label = "Try again";
  if (state?.kind === "success") {
    if (remaining === 0) label = "Queued";
    else if (!fresh) label = `Retry the rest ${formatCount(remaining)}`;
  }
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      aria-disabled={disabled}
      className="my-2 ml-12 sm:ml-0"
      onClick={() => {
        if (!disabled) onRetry(remaining);
      }}
    >
      {label}
    </Button>
  );
}

export function BulkRetryFeedback({
  state,
  noun,
  paused = false,
}: Readonly<{
  state: BulkRetryState | undefined;
  noun: "files" | "books";
  paused?: boolean;
}>): ReactElement | null {
  if (state === undefined || state.kind === "pending") return null;
  let message: string;
  if (state.kind === "failure") {
    if (state.cause === "forbidden")
      message = `This account cannot retry ${noun}. Sign in as an administrator.`;
    else if (state.cause === "session")
      message = `Your session has ended. Sign in again to retry ${noun}.`;
    else
      message = `Could not confirm whether these ${noun} were queued. Refresh the group before trying again.`;
  } else if (state.queued === 0 && state.remaining === 0) {
    message = "Nothing to retry. This group changed after the page loaded.";
  } else {
    message = `Queued ${formatCount(state.queued)}`;
    if (state.remaining > 0) message += `, ${formatCount(state.remaining)} remaining`;
    if (paused && noun === "files" && state.queued > 0)
      message +=
        ". Ingestion is paused, so they will run once it resumes. They return here if they fail again.";
  }
  return (
    <p
      role={state.kind === "failure" ? "alert" : "status"}
      className="text-fg-muted px-4 pb-3 pl-12 text-[13px]"
    >
      {message}
    </p>
  );
}
