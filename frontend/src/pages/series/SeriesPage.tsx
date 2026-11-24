/**
 * Production `/series/:id` page.
 *
 * Lists recorded work metadata in series order.
 */
import {
  graphql,
  useFragment,
  usePaginationFragment,
  usePreloadedQuery,
  useQueryLoader,
  type PreloadedQuery,
} from "react-relay";
import type { SeriesPageQuery } from "@/lib/relay/__generated__/SeriesPageQuery.graphql";
import type { SeriesPage_series$key } from "@/lib/relay/__generated__/SeriesPage_series.graphql";
import type { SeriesPage_works$key } from "@/lib/relay/__generated__/SeriesPage_works.graphql";
import type { SeriesPageWorksPaginationQuery } from "@/lib/relay/__generated__/SeriesPageWorksPaginationQuery.graphql";
import type { SeriesPage_work$key } from "@/lib/relay/__generated__/SeriesPage_work.graphql";
import type { SeriesPage_authors$key } from "@/lib/relay/__generated__/SeriesPage_authors.graphql";
import type { SeriesPageAuthorsPaginationQuery } from "@/lib/relay/__generated__/SeriesPageAuthorsPaginationQuery.graphql";
import { useEffect, useState, Suspense, type ReactElement } from "react";
import { Link, useLoaderData } from "react-router";

import { NotFound } from "@/components/RootErrorBoundary";
import { Skeleton } from "@/components/ui/skeleton";

import type { loader } from "@/routes/series";

const seriesPageQuery = graphql`
  query SeriesPageQuery($id: ID!) @throwOnFieldError {
    series(id: $id) {
      ...SeriesPage_series
    }
  }
`;

const seriesFragment = graphql`
  fragment SeriesPage_series on Series @throwOnFieldError {
    name
    ...SeriesPage_works
  }
`;

const seriesWorks = graphql`
  fragment SeriesPage_works on Series
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 50 }, cursor: { type: "String" })
  @refetchable(queryName: "SeriesPageWorksPaginationQuery", preferFetchable: true) {
    works(first: $count, after: $cursor) @connection(key: "SeriesPage_works") {
      edges {
        position
        node {
          id
          ...SeriesPage_work
        }
      }
    }
  }
`;

const seriesWork = graphql`
  fragment SeriesPage_work on Work @throwOnFieldError {
    title
    ...SeriesPage_authors
    manifestations(first: 1) {
      edges {
        node {
          id
          coverUrl
        }
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
`;

const seriesAuthors = graphql`
  fragment SeriesPage_authors on Work
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 20 }, cursor: { type: "String" })
  @refetchable(queryName: "SeriesPageAuthorsPaginationQuery", preferFetchable: true) {
    id
    authors(first: $count, after: $cursor) @connection(key: "SeriesPage_authors") {
      edges {
        node {
          id
          name
        }
      }
    }
  }
`;

export function SeriesPage(): ReactElement | null {
  const prepared = useLoaderData<typeof loader>();
  const [reference] = useQueryLoader<SeriesPageQuery>(seriesPageQuery, prepared);
  if (reference == null || prepared === null) return null;
  return (
    <Suspense fallback={<SeriesSkeleton />}>
      <SeriesContent reference={reference} />
    </Suspense>
  );
}

function SeriesContent({
  reference,
}: Readonly<{ reference: PreloadedQuery<SeriesPageQuery> }>): ReactElement {
  const query = usePreloadedQuery<SeriesPageQuery>(seriesPageQuery, reference);
  const data = useFragment<SeriesPage_series$key>(seriesFragment, query.series);
  const {
    data: works,
    hasNext,
    isLoadingNext,
    loadNext,
  } = usePaginationFragment<SeriesPageWorksPaginationQuery, SeriesPage_works$key>(
    seriesWorks,
    data,
  );
  const [error, setError] = useState<Error | null>(null);
  useEffect(() => {
    if (hasNext && !isLoadingNext) loadNext(50, { onComplete: setError });
  }, [hasNext, isLoadingNext, loadNext]);
  if (error !== null) throw error;
  if (data == null) return <NotFound />;
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
          <SeriesWorkRow key={edge.node.id} work={edge.node} position={edge.position ?? null} />
        ))}
      </ol>
    </div>
  );
}

function SeriesWorkRow({
  work: workKey,
  position,
}: Readonly<{ work: SeriesPage_work$key; position: number | null }>): ReactElement {
  const work = useFragment(seriesWork, workKey);
  const edition = work.manifestations.edges.at(0)?.node;
  const { data, hasNext, isLoadingNext, loadNext } = usePaginationFragment<
    SeriesPageAuthorsPaginationQuery,
    SeriesPage_authors$key
  >(seriesAuthors, work);
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
        <p className="text-fg-muted text-xs">{position == null ? "" : `#${String(position)}`}</p>
        <Link
          to={`/b/${edition?.id ?? ""}`}
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
