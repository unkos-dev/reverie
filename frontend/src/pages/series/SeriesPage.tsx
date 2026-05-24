/**
 * Production `/series/:id` page.
 *
 * Lists the works in a series in ordered position, surfacing a
 * completeness indicator (`own / total`) and a "gap" badge on works
 * the caller owns no manifestation of.
 */
import { useSuspenseQuery } from "@tanstack/react-query";
import { Suspense, type ReactElement } from "react";
import { Link, useParams } from "react-router";

import { getSeries, type SeriesDetail } from "@/api";
import { Skeleton } from "@/components/ui/skeleton";
import { queryKeys } from "@/lib/query/keys";

/**
 * Top-level page component. The `<Suspense>` boundary covers the
 * cold-cache path; the route loader pre-warms the cache so render is
 * synchronous on direct navigation.
 */
export function SeriesPage(): ReactElement {
  return (
    <Suspense fallback={<SeriesSkeleton />}>
      <SeriesContent />
    </Suspense>
  );
}

function SeriesContent(): ReactElement {
  const params = useParams<{ id: string }>();
  const id = params.id ?? "";
  const { data } = useSuspenseQuery<SeriesDetail>({
    queryKey: queryKeys.series.detail(id),
    queryFn: ({ signal }) => getSeries(id, signal),
  });

  const total = data.works.length;
  const owned = data.works.filter((w) => w.manifestations.length > 0).length;

  return (
    <div className="mx-auto max-w-5xl px-6 py-10 sm:px-10">
      <header className="mb-8">
        <p className="text-fg-muted text-xs uppercase tracking-wider">Series</p>
        <h1 className="font-display mt-1 text-3xl font-semibold tracking-tight text-fg">
          {data.name}
        </h1>
        <p className="text-fg-muted mt-2 text-sm">
          {owned} of {total} owned
        </p>
      </header>
      <ol className="space-y-3">
        {data.works.map((work) => {
          const cover = work.manifestations[0]?.cover_url;
          const owns = work.manifestations.length > 0;
          const positionLabel = work.position === null ? "" : `#${String(work.position)}`;
          return (
            <li
              key={work.id}
              className="border-border bg-surface-1 flex items-center gap-4 rounded-lg border p-4"
            >
              {typeof cover === "string" ? (
                <img
                  src={cover}
                  alt=""
                  className="border-border h-16 w-12 rounded-sm border object-cover"
                />
              ) : (
                <div className="border-border bg-surface-2 h-16 w-12 rounded-sm border" />
              )}
              <div className="min-w-0 flex-1">
                <p className="text-fg-muted text-xs">{positionLabel}</p>
                {owns ? (
                  <Link
                    to={`/b/${work.manifestations[0]?.id ?? ""}`}
                    className="text-fg block truncate text-base font-medium hover:underline"
                  >
                    {work.title}
                  </Link>
                ) : (
                  <p className="text-fg block truncate text-base font-medium">{work.title}</p>
                )}
                <p className="text-fg-muted truncate text-sm">{work.authors.join(", ")}</p>
              </div>
              {!owns && (
                <span className="text-fg-muted border-border rounded-full border px-2 py-0.5 text-xs">
                  Missing
                </span>
              )}
            </li>
          );
        })}
      </ol>
    </div>
  );
}

function SeriesSkeleton(): ReactElement {
  return (
    <div className="mx-auto max-w-5xl px-6 py-10 sm:px-10">
      <Skeleton className="h-8 w-1/3" />
      <Skeleton className="mt-3 h-4 w-1/4" />
      <div className="mt-8 space-y-3">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-20 w-full" />
        ))}
      </div>
    </div>
  );
}
