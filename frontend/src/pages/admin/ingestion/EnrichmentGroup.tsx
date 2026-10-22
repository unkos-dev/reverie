import { type ReactElement, useEffect, useRef, useState } from "react";
import { Link } from "react-router";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";

import { ApiError } from "@/api";
import {
  listEnrichmentFailures,
  triggerEnrichment,
  retryEnrichmentFailures,
} from "@/api/enrichment-failures";
import type { FailureClass, FailureItem } from "@/api/enrichment-failures";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableRow } from "@/components/ui/table";
import { queryKeys } from "@/lib/query/keys";

import { pluralise } from "./format";
import { GroupFooter } from "./GroupFooter";
import { GroupFailed, GroupLoading } from "./GroupState";
import { GroupShell, BulkRetryButton, BulkRetryFeedback, type BulkRetryState } from "./GroupShell";
import { alsoLine, FAILURE_CLASS_COPY, failureAlsoLabel, failureGroupTitle } from "./reason-copy";
import { usePagedGroup } from "./use-paged-group";

type EnrichmentGroupProps = {
  source: string | null;
  failureClass: FailureClass;
  count: number;
  defaultOpen: boolean;
  countsVersion: number;
  onRetain: () => void;
};

function statusText(item: FailureItem): string {
  const word = item.status === "skipped" ? "Stopped retrying" : "Will retry";
  return `${word}, tried ${pluralise(item.attempt_count, "time")}`;
}

function QueuedStatus(): ReactElement {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const active = document.activeElement;
    if (active === null || active === document.body) ref.current?.focus();
    return undefined;
  }, []);
  return (
    <span ref={ref} tabIndex={-1} className="text-fg-muted text-[13px]" role="status">
      Queued. It leaves this list once it runs.
    </span>
  );
}

function attemptKey(item: FailureItem): string {
  return `${item.attempted_at ?? ""}|${String(item.attempt_count)}`;
}

function RetryControl({ item }: Readonly<{ item: FailureItem }>): ReactElement {
  const queryClient = useQueryClient();
  const mutation = useMutation({
    mutationFn: (_attempt: string) => triggerEnrichment(item.manifestation_id),
    onSuccess: () => {
      // Marked stale without refetching now: the trigger resets the book to
      // pending, so an immediate refetch would drop the row and its
      // confirmation. The lists refresh on the next focus or visit.
      void queryClient.invalidateQueries({
        queryKey: queryKeys.enrichmentFailures.all,
        refetchType: "none",
      });
      void queryClient.invalidateQueries({ queryKey: queryKeys.dashboard.stats() });
    },
  });

  if (mutation.isSuccess && mutation.variables === attemptKey(item)) {
    return <QueuedStatus />;
  }

  return (
    <span className="inline-flex items-center gap-2">
      {mutation.isError ? (
        <span className="text-[13px]" role="alert">
          Could not queue it.
        </span>
      ) : null}
      <Button
        type="button"
        variant="outline"
        size="sm"
        isPending={mutation.isPending}
        onPress={() => {
          mutation.mutate(attemptKey(item));
        }}
      >
        {mutation.isPending ? (
          <Loader2 aria-hidden="true" className="motion-safe:animate-spin" />
        ) : null}
        {mutation.isPending ? "Queuing" : "Try again"}
      </Button>
    </span>
  );
}

function FailureRow({ item }: Readonly<{ item: FailureItem }>): ReactElement {
  return (
    <TableRow className="border-border hover:bg-transparent">
      <TableCell className="py-2.5 pl-12 align-top whitespace-normal">
        <Link
          to={`/b/${item.manifestation_id}`}
          className="font-bold underline underline-offset-[3px]"
        >
          {item.title}
        </Link>
        <span className="text-fg-muted ml-2 text-[13px]">{statusText(item)}</span>
        {item.also.length > 0 ? (
          <span className="text-fg-muted mt-0.5 block text-[13px]">
            {alsoLine(item.also.map(failureAlsoLabel))}
          </span>
        ) : null}
      </TableCell>
      <TableCell className="py-2.5 pr-4 text-right align-top">
        <RetryControl item={item} />
      </TableCell>
    </TableRow>
  );
}

function EnrichmentGroupBody({
  resetOn,
  source,
  failureClass,
  count,
}: Readonly<
  Pick<EnrichmentGroupProps, "source" | "failureClass" | "count"> & { resetOn?: object }
>): ReactElement {
  const group = usePagedGroup<FailureItem>({
    resetOn,
    firstKey: queryKeys.enrichmentFailures.groupFirst(source, failureClass),
    afterKey: (cursor) => queryKeys.enrichmentFailures.groupAfter(source, failureClass, cursor),
    fetchPage: (cursor, signal) =>
      listEnrichmentFailures(
        cursor === undefined
          ? { source, class: failureClass }
          : { source, class: failureClass, cursor },
        signal,
      ),
    getId: (item) => item.manifestation_id,
    refetchInterval: false,
    refetchOnWindowFocus: true,
  });

  if (group.isPending) return <GroupLoading label="Loading books" />;
  if (group.isError)
    return <GroupFailed message="Could not load these books." onRetry={group.retry} />;

  return (
    <>
      <Table aria-label={`Books: ${failureGroupTitle({ source, class: failureClass })}`}>
        <TableBody>
          {group.rows.map((item) => (
            <FailureRow key={item.manifestation_id} item={item} />
          ))}
        </TableBody>
      </Table>
      <GroupFooter group={group} total={count} noun="books" />
    </>
  );
}

function EnrichmentGroup({
  source,
  failureClass,
  count,
  defaultOpen,
  countsVersion,
  onRetain,
}: Readonly<EnrichmentGroupProps>): ReactElement {
  const client = useQueryClient();
  const [state, setState] = useState<BulkRetryState>();
  const pending = useRef(false);
  const forbidden = useRef(false);
  const mutation = useMutation({
    mutationFn: () => retryEnrichmentFailures(source, failureClass),
    retry: false,
    onSuccess: async (result) => {
      const continuation = {
        queryKey: queryKeys.enrichmentFailures.groupAfterPrefix(source, failureClass),
      };
      await client.cancelQueries(continuation);
      client.removeQueries(continuation);
      pending.current = false;
      const version =
        client.getQueryState(queryKeys.enrichmentFailures.counts())?.dataUpdatedAt ?? 0;
      setState({ kind: "success", ...result, countsVersion: version });
      void client.invalidateQueries({
        queryKey: queryKeys.enrichmentFailures.all,
        refetchType: "none",
      });
      void client.invalidateQueries({ queryKey: queryKeys.dashboard.stats() });
    },
    onError: (error) => {
      pending.current = false;
      let cause: Extract<BulkRetryState, { kind: "failure" }>["cause"] = "failed";
      if (error instanceof ApiError) {
        if (error.status === 403) cause = "forbidden";
        else if (error.status === 401) cause = "session";
      }
      if (cause === "forbidden") forbidden.current = true;
      setState({ kind: "failure", cause });
    },
  });
  return (
    <GroupShell
      headerControl={
        <BulkRetryButton
          state={state}
          count={count}
          countsVersion={countsVersion}
          noun="book"
          onRetry={(remaining) => {
            if (pending.current || forbidden.current) return;
            pending.current = true;
            onRetain();
            setState({ kind: "pending", count: remaining });
            mutation.mutate();
          }}
        />
      }
      feedback={<BulkRetryFeedback state={state} noun="books" />}
      title={failureGroupTitle({ source, class: failureClass })}
      action={FAILURE_CLASS_COPY[failureClass].detail}
      countLabel={pluralise(count, "book")}
      defaultOpen={defaultOpen}
    >
      <EnrichmentGroupBody
        resetOn={state?.kind === "success" ? state : undefined}
        source={source}
        failureClass={failureClass}
        count={count}
      />
    </GroupShell>
  );
}

export { EnrichmentGroup };
