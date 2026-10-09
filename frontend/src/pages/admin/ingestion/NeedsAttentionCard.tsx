import { type ReactElement, useState } from "react";
import { Info } from "lucide-react";

import { INPUT_REASONS } from "@/api/ingestion";
import type { InputCounts, InputReason } from "@/api/ingestion";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";

import { pluralise } from "./format";
import { InputGroup } from "./InputGroup";

type NeedsAttentionCardProps = {
  counts: InputCounts | undefined;
  isPending: boolean;
  onRetry: () => void;
  pollInterval: number | false;
  afterScan: boolean;
};

type Group = { reason: InputReason; count: number };

function CardHead({ children, total }: { children?: ReactElement; total?: string }): ReactElement {
  return (
    <div className="px-4 pt-4 pb-3">
      <h2
        id="needs-attention"
        className="flex items-baseline gap-2.5 text-base leading-snug font-bold"
      >
        Needs attention
        {total ? (
          <span className="text-fg-muted font-mono text-xs font-normal">{total}</span>
        ) : null}
      </h2>
      {children}
    </div>
  );
}

function groupsFrom(counts: InputCounts): Group[] {
  return INPUT_REASONS.flatMap((reason) => {
    const count = counts.by_reason.find((c) => c.reason === reason)?.count ?? 0;
    return count > 0 ? [{ reason, count }] : [];
  });
}

function NeedsAttentionCard({
  counts,
  isPending,
  onRetry,
  pollInterval,
  afterScan,
}: NeedsAttentionCardProps): ReactElement {
  const [openOnLoad, setOpenOnLoad] = useState<InputReason | null | undefined>(undefined);
  const groups = counts === undefined ? [] : groupsFrom(counts);
  const attention = groups.filter((g) => g.reason !== "format_not_accepted");
  if (openOnLoad === undefined && counts !== undefined)
    setOpenOnLoad(attention.at(0)?.reason ?? null);

  if (counts === undefined && isPending) {
    return (
      <Card role="region" aria-labelledby="needs-attention" aria-busy="true" className="gap-0 py-0">
        <CardHead />
        <div className="grid gap-5 px-4 pt-1 pb-5">
          <span role="status" className="sr-only">
            Loading files that need attention
          </span>
          <Skeleton className="bg-surface-3 h-3 w-1/2" />
          <Skeleton className="bg-surface-3 h-3 w-2/3" />
          <Skeleton className="bg-surface-3 h-3 w-2/5" />
        </div>
      </Card>
    );
  }

  if (counts === undefined) {
    return (
      <Card role="region" aria-labelledby="needs-attention" className="gap-0 py-0">
        <CardHead />
        <div className="flex items-start gap-3 px-4 pt-1 pb-5">
          <Info aria-hidden="true" className="text-fg-muted mt-0.5 size-4 shrink-0" />
          <div>
            <strong className="font-bold">Could not load this list.</strong>
            <p className="text-fg-muted text-[13px]">Your files are untouched. Try again.</p>
            <Button type="button" variant="outline" size="sm" className="mt-2" onClick={onRetry}>
              Try again
            </Button>
          </div>
        </div>
      </Card>
    );
  }

  const ignored = groups.find((g) => g.reason === "format_not_accepted");
  const renderGroup = (group: Group): ReactElement => (
    <InputGroup
      key={group.reason}
      reason={group.reason}
      count={group.count}
      defaultOpen={group.reason === openOnLoad}
      pollInterval={pollInterval}
    />
  );

  if (attention.length === 0) {
    return (
      <Card role="region" aria-labelledby="needs-attention" className="gap-0 py-0">
        <CardHead>
          <p className="text-fg-muted mt-0.5 max-w-[76ch] text-[13px]">
            Files Reverie could not import, grouped by why.
          </p>
        </CardHead>
        <div className="px-4 pt-1 pb-6">
          <strong className="font-bold">Nothing needs attention.</strong>
          <p className="text-fg-muted text-[13px]">
            Every file Reverie has seen was imported, is waiting its turn, or was ignored.
          </p>
        </div>
        {ignored ? renderGroup(ignored) : null}
      </Card>
    );
  }

  return (
    <Card role="region" aria-labelledby="needs-attention" className="gap-0 py-0">
      <CardHead
        total={`${pluralise(counts.attention_total, "file")}, ${pluralise(attention.length, "reason")}`}
      >
        <p className="text-fg-muted mt-0.5 max-w-[76ch] text-[13px]">
          Files Reverie could not import, grouped by why. Each original stays in the ingestion
          folder, unchanged.
        </p>
      </CardHead>
      {afterScan ? (
        <p className="text-fg-muted px-4 pb-3 text-[13px]">
          Files marked Needs a change were sent for another try. They return here if they fail
          again. Rejected files stay until the file changes.
        </p>
      ) : null}
      {groups.map(renderGroup)}
    </Card>
  );
}

export { NeedsAttentionCard };
