import { type ReactElement, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Info } from "lucide-react";

import { getEnrichmentFailureCounts } from "@/api/enrichment-failures";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { queryKeys } from "@/lib/query/keys";

import { pluralise } from "./format";
import { EnrichmentGroup } from "./EnrichmentGroup";

function CardHead({ total, children }: { total?: string; children?: ReactElement }): ReactElement {
  return (
    <div className="px-4 pt-4 pb-3">
      <h2
        id="enrichment-problems"
        className="flex items-baseline gap-2.5 text-base leading-snug font-bold"
      >
        Enrichment problems
        {total ? (
          <span className="text-fg-muted font-mono text-xs font-normal">{total}</span>
        ) : null}
      </h2>
      {children}
    </div>
  );
}

function groupKey(source: string | null | undefined, failureClass: string): string {
  return `${source ?? ""}/${failureClass}`;
}

/** Books whose enrichment is failing, grouped by source and class. Refetches on window focus only. */
function EnrichmentCard(): ReactElement {
  const counts = useQuery({
    queryKey: queryKeys.enrichmentFailures.counts(),
    queryFn: ({ signal }) => getEnrichmentFailureCounts(signal),
  });
  const [openOnLoad, setOpenOnLoad] = useState<string | null | undefined>(undefined);
  const groups = (counts.data?.by_failure ?? []).filter((g) => g.count > 0);
  if (openOnLoad === undefined && counts.data !== undefined) {
    const first = groups.at(0);
    setOpenOnLoad(first === undefined ? null : groupKey(first.source, first.class));
  }

  if (counts.data === undefined && counts.isPending) {
    return (
      <Card
        role="region"
        aria-labelledby="enrichment-problems"
        aria-busy="true"
        className="gap-0 py-0"
      >
        <CardHead />
        <div className="grid gap-4 px-4 pt-1 pb-5">
          <span role="status" className="sr-only">
            Loading books with enrichment problems
          </span>
          <Skeleton className="bg-surface-3 motion-reduce:animate-none h-3 w-3/5" />
          <Skeleton className="bg-surface-3 motion-reduce:animate-none h-3 w-4/5" />
          <Skeleton className="bg-surface-3 motion-reduce:animate-none h-3 w-2/5" />
        </div>
      </Card>
    );
  }

  if (counts.data === undefined) {
    return (
      <Card role="region" aria-labelledby="enrichment-problems" className="gap-0 py-0">
        <CardHead />
        <div className="flex items-start gap-3 px-4 pt-1 pb-5">
          <Info aria-hidden="true" className="text-fg-muted mt-0.5 size-4 shrink-0" />
          <div>
            <strong className="font-bold">Could not load this list.</strong>
            <p className="text-fg-muted text-[13px]">Your books are untouched. Try again.</p>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="mt-2"
              onClick={() => {
                void counts.refetch();
              }}
            >
              Try again
            </Button>
          </div>
        </div>
      </Card>
    );
  }

  if (groups.length === 0) {
    return (
      <Card role="region" aria-labelledby="enrichment-problems" className="gap-0 py-0">
        <CardHead />
        <div className="px-4 pt-1 pb-6">
          <strong className="font-bold">No enrichment problems.</strong>
          <p className="text-fg-muted text-[13px]">
            Every book was enriched, or is waiting its turn.
          </p>
        </div>
      </Card>
    );
  }

  return (
    <Card role="region" aria-labelledby="enrichment-problems" className="gap-0 py-0">
      <CardHead
        total={`${pluralise(counts.data.total, "book")}, ${pluralise(groups.length, "reason")}`}
      >
        <p className="text-fg-muted mt-0.5 mb-0 max-w-[76ch] pb-0 text-[13px]">
          Books where the metadata sources could not be reached or returned nothing usable, grouped
          by source and reason. Each book is listed once, under its main reason.
        </p>
      </CardHead>
      {groups.map((group) => {
        const key = groupKey(group.source, group.class);
        return (
          <EnrichmentGroup
            key={key}
            source={group.source ?? null}
            failureClass={group.class}
            count={group.count}
            defaultOpen={key === openOnLoad}
          />
        );
      })}
    </Card>
  );
}

export { EnrichmentCard };
