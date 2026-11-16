/** Book detail renders its prepared Relay reference and complete visible relations. */
import {
  useFragment,
  usePaginationFragment,
  usePreloadedQuery,
  useQueryLoader,
  useSubscribeToInvalidationState,
  type PreloadedQuery,
} from "react-relay";
import { decodeGraphQLId } from "@/api/graphql";
import {
  readerDetailBook,
  readerDetailQuery,
  readerWorkAuthors,
  readerDetailTags,
} from "@/lib/relay/reader";
import type { readerDetailQuery as DetailQuery } from "@/lib/relay/__generated__/readerDetailQuery.graphql";
import type { readerDetailBook$key } from "@/lib/relay/__generated__/readerDetailBook.graphql";
import type { readerWorkAuthors$key } from "@/lib/relay/__generated__/readerWorkAuthors.graphql";
import type { readerWorkAuthorsPaginationQuery as AuthorsPagination } from "@/lib/relay/__generated__/readerWorkAuthorsPaginationQuery.graphql";
import type { readerDetailTags$key } from "@/lib/relay/__generated__/readerDetailTags.graphql";
import type { readerDetailTagsPaginationQuery as TagsPagination } from "@/lib/relay/__generated__/readerDetailTagsPaginationQuery.graphql";
import { useEffect, Suspense, useState, useTransition, type ReactElement } from "react";
import { Link, useLoaderData } from "react-router";

import { type BookDetail } from "@/api";
import { CoverArtwork } from "@/components/CoverArtwork";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabPanel, TabList, Tab } from "@/components/ui/tabs";

import { VersionsTab } from "./VersionsTab";

import type { loader } from "@/routes/book";

/**
 * Outer page component. Splits suspense boundary from the data layer
 * so the skeleton is rendered with no chance of state pollution from
 * a previous book id.
 */
export function BookPage(): ReactElement | null {
  const prepared = useLoaderData<typeof loader>();
  if (prepared === null) return null;
  return (
    <Suspense fallback={<BookSkeleton />}>
      <BookContent reference={prepared.reference} />
    </Suspense>
  );
}

type DisplayBook = Pick<
  BookDetail,
  | "id"
  | "title"
  | "authors"
  | "series"
  | "cover_url"
  | "description"
  | "language"
  | "isbn_13"
  | "isbn_10"
  | "isbn_mismatch"
  | "publisher"
  | "pub_date"
  | "tags"
> & {
  ingestion_status: string;
  enrichment_status: string;
  validation_status: string;
  metadata_version_summary: { pending: number };
};

function BookContent({ reference }: { reference: PreloadedQuery<DetailQuery> }): ReactElement {
  const [current, load] = useQueryLoader<DetailQuery>(readerDetailQuery, reference);
  const query = usePreloadedQuery<DetailQuery>(readerDetailQuery, current ?? reference);
  const data = useFragment<readerDetailBook$key>(readerDetailBook, query.manifestation);
  const {
    data: authorData,
    hasNext: hasAuthors,
    isLoadingNext: loadingAuthors,
    loadNext: loadAuthors,
  } = usePaginationFragment<AuthorsPagination, readerWorkAuthors$key>(
    readerWorkAuthors,
    data?.work,
  );
  const {
    data: tagData,
    hasNext: hasTags,
    isLoadingNext: loadingTags,
    loadNext: loadTags,
  } = usePaginationFragment<TagsPagination, readerDetailTags$key>(readerDetailTags, data);
  const [pageError, setPageError] = useState<Error | null>(null);
  const [, startTransition] = useTransition();
  useSubscribeToInvalidationState(
    [
      ...(data === null || data === undefined ? [] : [data.id]),
      ...(data?.work == null ? [] : [data.work.id]),
    ],
    () => {
      startTransition(() => {
        load(reference.variables, { fetchPolicy: "network-only" });
      });
    },
  );
  useEffect(() => {
    if (hasAuthors && !loadingAuthors) loadAuthors(20, { onComplete: setPageError });
  }, [hasAuthors, loadingAuthors, loadAuthors]);
  useEffect(() => {
    if (hasTags && !loadingTags) loadTags(20, { onComplete: setPageError });
  }, [hasTags, loadingTags, loadTags]);
  if (pageError !== null) throw pageError;
  if (data == null || data.work == null) throw new Error("Book not found.");
  const series = data.work.selectedSeries;
  const book: DisplayBook = {
    id: decodeGraphQLId("Manifestation", data.id),
    title: data.work.title,
    authors: authorData?.authors.edges.map(({ node }) => node.name) ?? [],
    series:
      series == null
        ? null
        : {
            id: decodeGraphQLId("Series", series.node.id),
            name: series.node.name,
            position: series.position ?? null,
          },
    cover_url: data.coverUrl,
    description: data.work.description ?? null,
    language: data.work.language ?? null,
    isbn_13: data.isbn13 ?? null,
    isbn_10: data.isbn10 ?? null,
    isbn_mismatch: data.isbnMismatch,
    publisher: data.publisher ?? null,
    pub_date: data.publicationDate ?? null,
    ingestion_status: data.ingestionStatus,
    enrichment_status: data.enrichmentStatus,
    validation_status: data.validationStatus,
    tags: tagData?.tags.edges.map(({ node }) => node.name) ?? [],
    metadata_version_summary: { pending: data.versions.pendingCount },
  };

  const seriesLabel =
    book.series && book.series.position !== null
      ? `${book.series.name} · #${String(book.series.position)}`
      : (book.series?.name ?? null);

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
            <DetailCover book={book} />
          </div>
        </aside>
        <section className="min-w-0">
          <header className="mb-6">
            {book.series !== null && seriesLabel !== null ? (
              <p className="mb-2">
                {/* Series detail must be click-reachable from here —
                    it has no other inbound surface (spec §4). */}
                <Link
                  to={`/series/${book.series.id}`}
                  className="text-fg-muted hover:text-fg focus-visible:ring-accent rounded-sm text-xs uppercase tracking-[0.14em] transition-colors focus-visible:outline-none focus-visible:ring-2"
                >
                  {seriesLabel}
                </Link>
              </p>
            ) : null}
            <h1 className="font-display text-fg text-3xl font-semibold tracking-tight">
              {book.title}
            </h1>
            {book.authors.length > 0 ? (
              <p className="text-fg-muted mt-2 text-base">{book.authors.join(", ")}</p>
            ) : null}
          </header>

          <Tabs defaultSelectedKey="overview" className="w-full">
            <TabList aria-label="Book details">
              <Tab id="overview">Overview</Tab>
              <Tab id="versions">
                Versions
                {book.metadata_version_summary.pending > 0 ? (
                  <Badge variant="secondary" className="ml-2">
                    {book.metadata_version_summary.pending}
                  </Badge>
                ) : null}
              </Tab>
              <Tab id="activity">Activity</Tab>
            </TabList>

            <TabPanel id="overview">
              <OverviewTab book={book} />
            </TabPanel>
            <TabPanel id="versions">
              <VersionsTab book={book} />
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

type DetailCoverProps = {
  book: DisplayBook;
};

/**
 * Real cover art with the typographic spine fallback (spec S8) — the
 * detail page mirrors the grid tile so a missing or failing cover
 * never renders the browser's broken-image glyph.
 */
function DetailCover({ book }: Readonly<DetailCoverProps>): ReactElement {
  const [failed, setFailed] = useState(false);
  if (book.cover_url === "" || failed) {
    return <CoverArtwork bookId={book.id} title={book.title} authors={book.authors} />;
  }
  return (
    <img
      src={book.cover_url}
      alt={`Cover of ${book.title}`}
      loading="eager"
      decoding="async"
      onError={() => {
        setFailed(true);
      }}
      className="size-full object-cover"
    />
  );
}

type TabProps = {
  book: DisplayBook;
};

function OverviewTab({ book }: Readonly<TabProps>): ReactElement {
  return (
    <div className="space-y-6">
      {book.description !== null ? (
        <p className="text-fg leading-relaxed">{book.description}</p>
      ) : (
        <p className="text-fg-muted text-sm italic">No description yet.</p>
      )}
      <dl className="border-border grid grid-cols-2 gap-x-6 gap-y-3 border-t pt-6 text-sm">
        <Field label="Language" value={book.language ?? "—"} />
        <Field label="ISBN-13" value={book.isbn_13 ?? "—"} mono />
        <Field label="ISBN-10" value={book.isbn_10 ?? "—"} mono />
        {book.isbn_mismatch ? (
          <div className="col-span-2">
            <dt className="sr-only">ISBN mismatch</dt>
            <dd className="text-fg-muted text-sm">
              These ISBN-10 and ISBN-13 values do not correspond. Check both values.
            </dd>
          </div>
        ) : null}
        <Field label="Ingestion" value={book.ingestion_status} />
        <Field label="Enrichment" value={book.enrichment_status} />
        <Field label="Validation" value={book.validation_status} />
        {book.tags.length > 0 ? (
          <div className="col-span-2">
            <dt className="text-fg-muted mb-1 text-xs uppercase tracking-wide">Tags</dt>
            <dd className="flex flex-wrap gap-2">
              {book.tags.map((tag) => (
                <Badge key={tag} variant="outline">
                  {tag}
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
