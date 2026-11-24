/** Book detail renders its preloaded Relay reference and complete visible relations. */
import {
  graphql,
  useFragment,
  usePaginationFragment,
  usePreloadedQuery,
  useQueryLoader,
  useSubscribeToInvalidationState,
  type PreloadedQuery,
} from "react-relay";
import type { Key } from "react-aria-components";
import type { BookPageQuery } from "@/lib/relay/__generated__/BookPageQuery.graphql";
import type { BookPage_book$key } from "@/lib/relay/__generated__/BookPage_book.graphql";
import type { BookPage_cover$key } from "@/lib/relay/__generated__/BookPage_cover.graphql";
import type { BookPage_overview$key } from "@/lib/relay/__generated__/BookPage_overview.graphql";
import type { BookPage_authors$key } from "@/lib/relay/__generated__/BookPage_authors.graphql";
import type { BookPageAuthorsPaginationQuery } from "@/lib/relay/__generated__/BookPageAuthorsPaginationQuery.graphql";
import type { BookPage_tags$key } from "@/lib/relay/__generated__/BookPage_tags.graphql";
import type { BookPageTagsPaginationQuery } from "@/lib/relay/__generated__/BookPageTagsPaginationQuery.graphql";
import versionsTabQuery, {
  type VersionsTabQuery,
} from "@/lib/relay/__generated__/VersionsTabQuery.graphql";
import { useEffect, Suspense, useState, useTransition, type ReactElement } from "react";
import { Link, useLoaderData } from "react-router";

import { CoverArtwork } from "@/components/CoverArtwork";
import { NotFound } from "@/components/RootErrorBoundary";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabPanel, TabList, Tab } from "@/components/ui/tabs";

import { VersionsTab } from "./VersionsTab";
import type { loader } from "@/routes/book";

const bookPageQuery = graphql`
  query BookPageQuery($id: ID!) @throwOnFieldError {
    manifestation(id: $id) {
      id
      ...BookPage_book
    }
  }
`;

const bookFragment = graphql`
  fragment BookPage_book on Manifestation @throwOnFieldError {
    id
    work {
      id
      title
      ...BookPage_authors
      selectedSeries {
        node {
          id
          name
        }
        position
      }
    }
    ...BookPage_overview
    ...BookPage_cover
    ...VersionsTab_book
    versions {
      pendingCount
    }
  }
`;

const coverFragment = graphql`
  fragment BookPage_cover on Manifestation @throwOnFieldError {
    id
    coverUrl
  }
`;

const overviewFragment = graphql`
  fragment BookPage_overview on Manifestation @throwOnFieldError {
    isbn13
    isbn10
    isbnMismatch
    ingestionStatus
    validationStatus
    enrichmentStatus
    work {
      description
      language
    }
    ...BookPage_tags
  }
`;

const bookAuthors = graphql`
  fragment BookPage_authors on Work
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 20 }, cursor: { type: "String" })
  @refetchable(queryName: "BookPageAuthorsPaginationQuery", preferFetchable: true) {
    id
    detailAuthors: authors(first: $count, after: $cursor)
      @connection(key: "BookPage_detailAuthors") {
      edges {
        node {
          id
          name
        }
      }
    }
  }
`;

const bookTags = graphql`
  fragment BookPage_tags on Manifestation
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 20 }, cursor: { type: "String" })
  @refetchable(queryName: "BookPageTagsPaginationQuery", preferFetchable: true) {
    id
    tags(first: $count, after: $cursor) @connection(key: "BookPage_tags") {
      edges {
        node {
          id
          name
        }
      }
    }
  }
`;

export function BookPage(): ReactElement | null {
  const prepared = useLoaderData<typeof loader>();
  const [reference, load] = useQueryLoader<BookPageQuery>(bookPageQuery, prepared);
  if (reference == null || prepared === null) return null;
  return (
    <Suspense fallback={<BookSkeleton />}>
      <BookContent
        reference={reference}
        reload={(id) => {
          load({ id }, { fetchPolicy: "network-only" });
        }}
      />
    </Suspense>
  );
}

function BookContent({
  reference,
  reload,
}: Readonly<{
  reference: PreloadedQuery<BookPageQuery>;
  reload: (id: string) => void;
}>): ReactElement {
  const query = usePreloadedQuery<BookPageQuery>(bookPageQuery, reference);
  if (query.manifestation == null) return <NotFound />;
  return (
    <BookInformation key={query.manifestation.id} book={query.manifestation} reload={reload} />
  );
}

function BookInformation({
  book,
  reload,
}: Readonly<{ book: BookPage_book$key; reload: (id: string) => void }>): ReactElement {
  const data = useFragment(bookFragment, book);
  const {
    data: authorData,
    hasNext: hasAuthors,
    isLoadingNext: loadingAuthors,
    loadNext: loadAuthors,
  } = usePaginationFragment<BookPageAuthorsPaginationQuery, BookPage_authors$key>(
    bookAuthors,
    data.work,
  );
  const [pageError, setPageError] = useState<Error | null>(null);
  const [, startTransition] = useTransition();
  const [tab, setTab] = useState<Key>("overview");
  const [versions, loadVersions] = useQueryLoader<VersionsTabQuery>(versionsTabQuery);
  useSubscribeToInvalidationState(data.work == null ? [data.id] : [data.id, data.work.id], () => {
    startTransition(() => {
      reload(data.id);
    });
  });
  useEffect(() => {
    if (hasAuthors && !loadingAuthors) loadAuthors(20, { onComplete: setPageError });
  }, [hasAuthors, loadingAuthors, loadAuthors]);
  if (pageError !== null) throw pageError;
  if (data.work == null) throw new Error("Book not found.");
  const title = data.work.title;
  const authors = authorData?.detailAuthors.edges.map(({ node }) => node.name) ?? [];
  const series = data.work.selectedSeries;
  const seriesLabel =
    series?.position != null
      ? `${series.node.name} · #${String(series.position)}`
      : series?.node.name;
  const pendingVersions = data.versions.pendingCount;

  return (
    <div className="mx-auto max-w-6xl px-6 py-10 sm:px-10">
      <nav className="mb-6 text-sm">
        <Link to="/library" className="text-fg-muted hover:text-fg">
          ← Library
        </Link>
      </nav>
      <div className="grid grid-cols-1 gap-10 md:grid-cols-[280px_1fr]">
        <aside className="md:sticky md:top-10 md:self-start">
          <div className="border-border bg-surface relative aspect-[2/3] overflow-hidden rounded-md border">
            <DetailCover book={data} title={title} authors={authors} />
          </div>
        </aside>
        <section className="min-w-0">
          <header className="mb-6">
            {series != null && seriesLabel !== undefined ? (
              <p className="mb-2">
                {/* Series detail must be click-reachable from here —
                    it has no other inbound surface (spec §4). */}
                <Link
                  to={`/series/${series.node.id}`}
                  className="text-fg-muted hover:text-fg focus-visible:ring-accent rounded-sm text-xs uppercase tracking-[0.14em] transition-colors focus-visible:outline-none focus-visible:ring-2"
                >
                  {seriesLabel}
                </Link>
              </p>
            ) : null}
            <h1 className="font-display text-fg text-3xl font-semibold tracking-tight">{title}</h1>
            {authors.length > 0 ? (
              <p className="text-fg-muted mt-2 text-base">{authors.join(", ")}</p>
            ) : null}
          </header>

          <Tabs
            selectedKey={tab}
            onSelectionChange={(next) => {
              if (next === "versions") loadVersions({ id: data.id });
              setTab(next);
            }}
            className="w-full"
          >
            <TabList aria-label="Book details">
              <Tab id="overview">Overview</Tab>
              <Tab id="versions">
                Versions
                {pendingVersions > 0 ? (
                  <Badge variant="secondary" className="ml-2">
                    {pendingVersions}
                  </Badge>
                ) : null}
              </Tab>
              <Tab id="activity">Activity</Tab>
            </TabList>

            <TabPanel id="overview">
              <OverviewTab book={data} />
            </TabPanel>
            <TabPanel id="versions">
              <Suspense fallback={<Skeleton className="h-32 w-full" aria-busy="true" />}>
                {versions != null && <VersionsTab book={data} reference={versions} />}
              </Suspense>
            </TabPanel>
            <TabPanel id="activity">
              <p className="text-fg-muted text-sm">Activity timeline lands in a later sub-phase.</p>
            </TabPanel>
          </Tabs>
        </section>
      </div>
    </div>
  );
}

function DetailCover({
  book,
  title,
  authors,
}: Readonly<{
  book: BookPage_cover$key;
  title: string;
  authors: string[];
}>): ReactElement {
  const { id, coverUrl } = useFragment(coverFragment, book);
  const [failed, setFailed] = useState(false);
  if (coverUrl === "" || failed) {
    return <CoverArtwork bookId={id} title={title} authors={authors} />;
  }
  return (
    <img
      src={coverUrl}
      alt={`Cover of ${title}`}
      loading="eager"
      decoding="async"
      onError={() => {
        setFailed(true);
      }}
      className="size-full object-cover"
    />
  );
}

function OverviewTab({ book }: Readonly<{ book: BookPage_overview$key }>): ReactElement {
  const data = useFragment(overviewFragment, book);
  const {
    data: tagData,
    hasNext,
    isLoadingNext,
    loadNext,
  } = usePaginationFragment<BookPageTagsPaginationQuery, BookPage_tags$key>(bookTags, data);
  const [pageError, setPageError] = useState<Error | null>(null);
  useEffect(() => {
    if (hasNext && !isLoadingNext) loadNext(20, { onComplete: setPageError });
  }, [hasNext, isLoadingNext, loadNext]);
  if (pageError !== null) throw pageError;
  const description = data.work?.description ?? null;
  const tags = tagData.tags.edges;
  return (
    <div className="space-y-6">
      {description !== null ? (
        <p className="text-fg leading-relaxed">{description}</p>
      ) : (
        <p className="text-fg-muted text-sm italic">No description yet.</p>
      )}
      <dl className="border-border grid grid-cols-2 gap-x-6 gap-y-3 border-t pt-6 text-sm">
        <Field label="Language" value={data.work?.language ?? "—"} />
        <Field label="ISBN-13" value={data.isbn13 ?? "—"} mono />
        <Field label="ISBN-10" value={data.isbn10 ?? "—"} mono />
        {data.isbnMismatch ? (
          <div className="col-span-2">
            <dt className="sr-only">ISBN mismatch</dt>
            <dd className="text-fg-muted text-sm">
              These ISBN-10 and ISBN-13 values do not correspond. Check both values.
            </dd>
          </div>
        ) : null}
        <Field label="Ingestion" value={data.ingestionStatus.toLowerCase()} />
        <Field label="Enrichment" value={data.enrichmentStatus.toLowerCase()} />
        <Field label="Validation" value={data.validationStatus.toLowerCase()} />
        {tags.length > 0 ? (
          <div className="col-span-2">
            <dt className="text-fg-muted mb-1 text-xs uppercase tracking-wide">Tags</dt>
            <dd className="flex flex-wrap gap-2">
              {tags.map(({ node: tag }) => (
                <Badge key={tag.id} variant="outline">
                  {tag.name}
                </Badge>
              ))}
            </dd>
          </div>
        ) : null}
      </dl>
    </div>
  );
}

type FieldProps = {
  label: string;
  value: string;
  mono?: boolean;
};

function Field({ label, value, mono = false }: Readonly<FieldProps>): ReactElement {
  return (
    <div>
      <dt className="text-fg-muted mb-1 text-xs uppercase tracking-wide">{label}</dt>
      <dd className={mono ? "text-fg font-mono text-xs" : "text-fg"}>{value}</dd>
    </div>
  );
}

function BookSkeleton(): ReactElement {
  return (
    <div className="mx-auto max-w-6xl px-6 py-10 sm:px-10" aria-busy="true">
      <Skeleton className="mb-6 h-5 w-24" />
      <div className="grid grid-cols-1 gap-10 md:grid-cols-[280px_1fr]">
        <Skeleton className="aspect-[2/3] w-full rounded-md" />
        <div className="space-y-4">
          <Skeleton className="h-8 w-3/4" />
          <Skeleton className="h-5 w-1/2" />
          <Skeleton className="h-32 w-full" />
        </div>
      </div>
    </div>
  );
}
