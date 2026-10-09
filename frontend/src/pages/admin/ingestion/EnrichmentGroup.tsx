import { type ReactElement } from "react";
import { Link } from "react-router";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";

import { listEnrichmentFailures, triggerEnrichment } from "@/api/enrichment-failures";
import type { FailureClass, FailureItem } from "@/api/enrichment-failures";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableRow } from "@/components/ui/table";
import { queryKeys } from "@/lib/query/keys";

import { pluralise } from "./format";
import { GroupFooter } from "./GroupFooter";
import { GroupFailed, GroupLoading } from "./GroupState";
import { GroupShell } from "./GroupShell";
import { FAILURE_CLASS_COPY, failureAlsoLabel, failureGroupTitle } from "./reason-copy";
import { usePagedGroup } from "./use-paged-group";

type EnrichmentGroupProps = {
  source: string | null;
  failureClass: FailureClass;
  count: number;
  defaultOpen: boolean;
};

function statusText(item: FailureItem): string {
  const word = item.status === "skipped" ? "Stopped retrying" : "Will retry";
  return `${word}, tried ${pluralise(item.attempt_count, "time")}`;
}

function RetryControl({ item }: { item: FailureItem }): ReactElement {
  const queryClient = useQueryClient();
  const mutation = useMutation({
    mutationFn: () => triggerEnrichment(item.manifestation_id),
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

  if (mutation.isSuccess) {
    return (
      <span className="text-fg-muted text-[13px]" role="status">
        Queued. It leaves this list once it runs.
      </span>
    );
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
        aria-disabled={mutation.isPending}
        className={mutation.isPending ? "pointer-events-none opacity-50" : undefined}
        onClick={() => {
          mutation.mutate();
        }}
      >
        {mutation.isPending ? <Loader2 aria-hidden="true" className="animate-spin" /> : null}
        {mutation.isPending ? "Queuing" : "Try again"}
      </Button>
    </span>
  );
}

function FailureRow({ item }: { item: FailureItem }): ReactElement {
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
            Also: {item.also.map(failureAlsoLabel).join("; ")}
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
  source,
  failureClass,
  count,
}: Omit<EnrichmentGroupProps, "defaultOpen">): ReactElement {
  const group = usePagedGroup<FailureItem>({
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
}: EnrichmentGroupProps): ReactElement {
  return (
    <GroupShell
      title={failureGroupTitle({ source, class: failureClass })}
      action={FAILURE_CLASS_COPY[failureClass].detail}
      countLabel={pluralise(count, "book")}
      defaultOpen={defaultOpen}
    >
      <EnrichmentGroupBody source={source} failureClass={failureClass} count={count} />
    </GroupShell>
  );
}

export { EnrichmentGroup };
