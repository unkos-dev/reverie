/**
 * Production `/series/:id` page.
 *
 * Lists recorded work metadata in series order.
 */
import {
  useFragment,
  usePaginationFragment,
  usePreloadedQuery,
  type PreloadedQuery,
} from "react-relay";
import { decodeGraphQLId } from "@/api/graphql";
import {
  readerSeriesQuery,
  readerSeries,
  readerSeriesWorks,
  readerWorkAuthors,
} from "@/lib/relay/reader";
import type { readerSeriesQuery as SeriesQuery } from "@/lib/relay/__generated__/readerSeriesQuery.graphql";
import type { readerSeries$key } from "@/lib/relay/__generated__/readerSeries.graphql";
import type {
  readerSeriesWorks$key,
  readerSeriesWorks$data,
} from "@/lib/relay/__generated__/readerSeriesWorks.graphql";
import type { readerSeriesWorksPaginationQuery as WorksPagination } from "@/lib/relay/__generated__/readerSeriesWorksPaginationQuery.graphql";
import type { readerWorkAuthors$key } from "@/lib/relay/__generated__/readerWorkAuthors.graphql";
import type { readerWorkAuthorsPaginationQuery as AuthorsPagination } from "@/lib/relay/__generated__/readerWorkAuthorsPaginationQuery.graphql";
import { useEffect, useState, Suspense, type ReactElement } from "react";
import { Link, useLoaderData } from "react-router";

import { Skeleton } from "@/components/ui/skeleton";

import type { loader } from "@/routes/series";

/**
 * Top-level page component. The `<Suspense>` boundary covers the
 * cold-cache path; the route loader pre-warms the cache so render is
 * synchronous on direct navigation.
 */
export function SeriesPage(): ReactElement | null {
  const prepared = useLoaderData<typeof loader>();
  useEffect(
    () => () => {
      prepared?.dispose();
    },
    [prepared],
  );
  if (prepared === null) return null;
  return (
    <Suspense fallback={<SeriesSkeleton />}>
      <SeriesContent reference={prepared.reference} />
    </Suspense>
  );
}

function SeriesContent({ reference }: { reference: PreloadedQuery<SeriesQuery> }): ReactElement {
  const query = usePreloadedQuery<SeriesQuery>(readerSeriesQuery, reference);
  const data = useFragment<readerSeries$key>(readerSeries, query.series);
  const {
    data: works,
    hasNext,
    isLoadingNext,
    loadNext,
  } = usePaginationFragment<WorksPagination, readerSeriesWorks$key>(readerSeriesWorks, data);
  const [error, setError] = useState<Error | null>(null);
  useEffect(() => {
    if (hasNext && !isLoadingNext) loadNext(50, { onComplete: setError });
  }, [hasNext, isLoadingNext, loadNext]);
  if (error !== null) throw error;
  if (data == null) throw new Error("Series not found.");
  return (
    <div className="mx-auto max-w-5xl px-6 py-10 sm:px-10">
      <header className="mb-8">
        <p className="text-fg-muted text-xs uppercase tracking-wider">Series</p>
        <h1 className="font-display mt-1 text-3xl font-semibold tracking-tight text-fg">
          {data.name}
        </h1>
      </header>
      <ol className="space-y-3">
        {works?.works.edges.map((edge) => (
          <SeriesWorkRow key={edge.node.id} edge={edge} />
        ))}
      </ol>
    </div>
  );
}

function SeriesWorkRow({
  edge,
}: {
  edge: readerSeriesWorks$data["works"]["edges"][number];
}): ReactElement {
  const work = edge.node;
  const edition = work.manifestations.edges.at(0)?.node;
  const { data, hasNext, isLoadingNext, loadNext } = usePaginationFragment<
    AuthorsPagination,
    readerWorkAuthors$key
  >(readerWorkAuthors, work);
  const [error, setError] = useState<Error | null>(null);
  useEffect(() => {
    if (hasNext && !isLoadingNext) loadNext(20, { onComplete: setError });
  }, [hasNext, isLoadingNext, loadNext]);
  if (error !== null) throw error;
  return (
    <li className="border-border bg-surface flex items-center gap-4 rounded-lg border p-4">
      {typeof edition?.coverUrl === "string" ? (
        <img
          src={edition.coverUrl}
          alt=""
          className="border-border h-16 w-12 rounded-sm border object-cover"
        />
      ) : (
        <div className="border-border bg-surface-2 h-16 w-12 rounded-sm border" />
      )}
      <div className="min-w-0 flex-1">
        <p className="text-fg-muted text-xs">
          {edge.position == null ? "" : `#${String(edge.position)}`}
        </p>
        <Link
          to={`/b/${decodeGraphQLId("Manifestation", edition?.id ?? "")}`}
          className="text-fg block truncate text-base font-medium hover:underline"
        >
          {work.title}
        </Link>
        <p className="text-fg-muted truncate text-sm">
          {data.authors.edges.map(({ node }) => node.name).join(", ")}
        </p>
      </div>
    </li>
  );
}

function SeriesSkeleton(): ReactElement {
  return (
    <div className="mx-auto max-w-5xl px-6 py-10 sm:px-10">
      <Skeleton className="h-8 w-1/3" />
      <div className="mt-8 space-y-3">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-20 w-full" />
        ))}
      </div>
    </div>
  );
}
