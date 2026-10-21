import { type ReactElement } from "react";
import { Info } from "lucide-react";
import { Link } from "react-router";

import { listInputs } from "@/api/ingestion";
import type { InputCounts, InputItem } from "@/api/ingestion";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { formatDateTime } from "@/lib/format/datetime";
import { queryKeys } from "@/lib/query/keys";

import { pluralise, splitPath } from "./format";
import { GroupFooter } from "./GroupFooter";
import { usePagedGroup } from "./use-paged-group";

type DuplicatesCardProps = {
  counts: InputCounts | undefined;
  countsPending: boolean;
  onRetryCounts: () => void;
  pollInterval: number | false;
};

function CardHead({ count }: Readonly<{ count?: number }>): ReactElement {
  return (
    <div className="px-4 pt-4 pb-3">
      <h2
        id="skipped-duplicates"
        className="flex items-baseline gap-2.5 text-base leading-snug font-bold"
      >
        Skipped as duplicates
        {count !== undefined ? (
          <span className="text-fg-muted font-mono text-xs font-normal">
            {pluralise(count, "file")}
          </span>
        ) : null}
      </h2>
      {count !== undefined ? (
        <p className="text-fg-muted mt-0.5 max-w-[76ch] text-[13px]">
          Files that match a book already in the library, so Reverie did not import them again.
          Delete them if you do not need them. Removing duplicate source files under Settings,
          Acquisition applies only to duplicates found from then on.
        </p>
      ) : null}
    </div>
  );
}

function LoadingCard(): ReactElement {
  return (
    <Card
      role="region"
      aria-labelledby="skipped-duplicates"
      aria-busy="true"
      className="gap-0 py-0"
    >
      <CardHead />
      <div className="grid gap-5 px-4 pt-1 pb-5">
        <span role="status" className="sr-only">
          Loading skipped duplicates
        </span>
        <Skeleton className="bg-surface-3 motion-reduce:animate-none h-3 w-1/2" />
        <Skeleton className="bg-surface-3 motion-reduce:animate-none h-3 w-2/3" />
        <Skeleton className="bg-surface-3 motion-reduce:animate-none h-3 w-2/5" />
      </div>
    </Card>
  );
}

function FailedCard({ onRetry }: Readonly<{ onRetry: () => void }>): ReactElement {
  return (
    <Card role="region" aria-labelledby="skipped-duplicates" className="gap-0 py-0">
      <CardHead />
      <div className="flex items-start gap-3 px-4 pt-1 pb-5">
        <Info aria-hidden="true" className="text-fg-muted mt-0.5 size-4 shrink-0" />
        <div>
          <div role="alert">
            <strong className="font-bold">Could not load this list.</strong>
            <p className="text-fg-muted text-[13px]">Your files are untouched. Try again.</p>
          </div>
          <Button type="button" variant="outline" size="sm" className="mt-2" onPress={onRetry}>
            Try again
          </Button>
        </div>
      </div>
    </Card>
  );
}

function DuplicateRow({ item }: Readonly<{ item: InputItem }>): ReactElement {
  const { dir, name } = splitPath(item.path);
  return (
    <TableRow className="border-border hover:bg-transparent">
      <TableCell className="py-2.5 pl-4 align-top whitespace-normal">
        <div className="font-mono text-[12.5px] break-all">
          <span className="text-fg-muted">{dir}</span>
          {name}
        </div>
      </TableCell>
      <TableCell className="py-2.5 align-top whitespace-normal text-[13px]">
        {item.matched ? (
          <Link
            to={`/b/${item.matched.manifestation_id}`}
            className="font-bold underline underline-offset-[3px]"
          >
            {item.matched.title}
          </Link>
        ) : (
          <span className="text-fg-muted">No matching book on record.</span>
        )}
      </TableCell>
      <TableCell className="text-fg-muted py-2.5 pr-4 text-right align-top text-[13px]">
        {item.completed_at ? formatDateTime(item.completed_at) : null}
      </TableCell>
    </TableRow>
  );
}

function DuplicateList({
  count,
  pollInterval,
}: Readonly<{ count: number; pollInterval: number | false }>): ReactElement {
  const group = usePagedGroup<InputItem>({
    firstKey: queryKeys.ingestion.groupFirst("duplicate"),
    afterKey: (cursor) => queryKeys.ingestion.groupAfter("duplicate", cursor),
    fetchPage: (cursor, signal) =>
      listInputs(
        cursor === undefined ? { reason: "duplicate" } : { reason: "duplicate", cursor },
        signal,
      ),
    getId: (item) => item.id,
    refetchInterval: pollInterval,
    refetchOnWindowFocus: true,
  });
  if (group.isPending) return <LoadingCard />;
  if (group.isError) return <FailedCard onRetry={group.retry} />;
  return (
    <Card role="region" aria-labelledby="skipped-duplicates" className="gap-0 py-0">
      <CardHead count={count} />
      <Table aria-labelledby="skipped-duplicates">
        <TableHeader>
          <TableRow className="border-border bg-surface-2 hover:bg-surface-2">
            <TableHead className="text-fg-muted pl-4 text-xs">File</TableHead>
            <TableHead className="text-fg-muted text-xs">Matches</TableHead>
            <TableHead className="text-fg-muted pr-4 text-right text-xs">Last tried</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {group.rows.map((item) => (
            <DuplicateRow key={item.id} item={item} />
          ))}
        </TableBody>
      </Table>
      <GroupFooter group={group} total={count} noun="files" />
    </Card>
  );
}

function DuplicatesCard({
  counts,
  countsPending,
  onRetryCounts,
  pollInterval,
}: Readonly<DuplicatesCardProps>): ReactElement {
  if (counts === undefined && countsPending) return <LoadingCard />;
  if (counts === undefined) return <FailedCard onRetry={onRetryCounts} />;
  const count = counts.by_reason.find((entry) => entry.reason === "duplicate")?.count ?? 0;
  if (count > 0) return <DuplicateList count={count} pollInterval={pollInterval} />;
  return (
    <Card role="region" aria-labelledby="skipped-duplicates" className="gap-0 py-0">
      <CardHead count={0} />
      <div className="px-4 pt-1 pb-6">
        <strong className="font-bold">No files were skipped as duplicates.</strong>
        <p className="text-fg-muted text-[13px]">
          If removal of duplicate source files is on under Settings, Acquisition, Reverie deletes a
          duplicate when it finds it, so it does not appear here.
        </p>
      </div>
    </Card>
  );
}

export { DuplicatesCard };
