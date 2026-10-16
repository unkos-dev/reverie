import { type ReactElement, type ReactNode } from "react";

import { listInputs } from "@/api/ingestion";
import type { InputItem, InputReason } from "@/api/ingestion";
import { Table, TableBody, TableCell, TableRow } from "@/components/ui/table";
import { formatDateTime } from "@/lib/format/datetime";
import { queryKeys } from "@/lib/query/keys";

import { pluralise, splitPath } from "./format";
import { GroupFooter } from "./GroupFooter";
import { GroupFailed, GroupLoading } from "./GroupState";
import { GroupShell } from "./GroupShell";
import { alsoLine, INPUT_REASON_COPY, inputGroupTitle } from "./reason-copy";
import { usePagedGroup } from "./use-paged-group";

type InputGroupProps = {
  headerControl?: ReactNode;
  feedback?: ReactNode;
  reason: InputReason;
  count: number;
  defaultOpen: boolean;
  pollInterval: number | false;
};

function InputRow({ item }: Readonly<{ item: InputItem }>): ReactElement {
  const { dir, name } = splitPath(item.path);
  const also = item.reasons.slice(1).map((r) => INPUT_REASON_COPY[r].label);
  return (
    <TableRow className="border-border hover:bg-transparent">
      <TableCell className="py-2.5 pl-12 align-top whitespace-normal">
        <div className="font-mono text-[12.5px] break-all">
          <span className="text-fg-muted">{dir}</span>
          {name}
        </div>
        {also.length > 0 ? (
          <span className="text-fg-muted mt-0.5 block text-[13px]">{alsoLine(also)}</span>
        ) : null}
      </TableCell>
      <TableCell className="text-fg-muted py-2.5 pr-4 text-right align-top text-[13px]">
        {item.completed_at ? formatDateTime(item.completed_at) : null}
      </TableCell>
    </TableRow>
  );
}

function InputGroupBody({
  reason,
  count,
  pollInterval,
}: Readonly<Omit<InputGroupProps, "defaultOpen">>): ReactElement {
  const group = usePagedGroup<InputItem>({
    firstKey: queryKeys.ingestion.groupFirst(reason),
    afterKey: (cursor) => queryKeys.ingestion.groupAfter(reason, cursor),
    fetchPage: (cursor, signal) =>
      listInputs(cursor === undefined ? { reason } : { reason, cursor }, signal),
    getId: (item) => item.id,
    refetchInterval: pollInterval,
    refetchOnWindowFocus: true,
  });

  if (group.isPending) return <GroupLoading label="Loading files" />;
  if (group.isError)
    return <GroupFailed message="Could not load these files." onRetry={group.retry} />;

  return (
    <>
      <Table aria-label={`Files: ${inputGroupTitle(reason)}`}>
        <TableBody>
          {group.rows.map((item) => (
            <InputRow key={item.id} item={item} />
          ))}
        </TableBody>
      </Table>
      <GroupFooter group={group} total={count} noun="files" />
    </>
  );
}

function InputGroup({
  headerControl,
  feedback,
  reason,
  count,
  defaultOpen,
  pollInterval,
}: Readonly<InputGroupProps>): ReactElement {
  const copy = INPUT_REASON_COPY[reason];
  return (
    <GroupShell
      headerControl={headerControl}
      feedback={feedback}
      title={inputGroupTitle(reason)}
      action={copy.action}
      tag={copy.tag}
      countLabel={pluralise(count, "file")}
      muted={reason === "format_not_accepted"}
      defaultOpen={defaultOpen}
    >
      <InputGroupBody reason={reason} count={count} pollInterval={pollInterval} />
    </GroupShell>
  );
}

export { InputGroup };
