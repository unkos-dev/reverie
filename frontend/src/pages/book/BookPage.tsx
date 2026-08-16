/**
 * Production `/b/:id` page.
 *
 * Sticky cover aside + Tabs layout. Data sourced from
 * `GET /api/v1/books/{id}` via `useSuspenseQuery`; the loader has already
 * primed the cache.
 *
 * The Versions tab is implemented in [`VersionsTab`] (sub-phase 11c):
 * per-field pending rows with accept / reject / revert affordances
 * plus a manual edit form (`PATCH /api/v1/books/{id}/metadata`).
 */
import { useSuspenseQuery } from "@tanstack/react-query";
import { Suspense, useState, type ReactElement } from "react";
import { Link, useParams } from "react-router";

import { getBook, type BookDetail } from "@/api";
import { CoverArtwork } from "@/components/CoverArtwork";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { queryKeys } from "@/lib/query/keys";

import { VersionsTab } from "./VersionsTab";

/**
 * Outer page component. Splits suspense boundary from the data layer
 * so the skeleton is rendered with no chance of state pollution from
 * a previous book id.
 */
export function BookPage(): ReactElement {
  const { id } = useParams<{ id: string }>();
  // Loader rejects a missing id before the component renders. The
  // narrow-or-throw here exists for the safety-belt case where this
  // component is rendered outside the routed flow (e.g. a direct
  // import in a test that forgets to mount `:id`); explicit failure
  // beats a `getBook("")` round-trip to the backend.
  if (typeof id !== "string" || id.length === 0) {
    throw new Error("BookPage requires the `id` route param.");
  }
  return (
    <Suspense fallback={<BookSkeleton />}>
      <BookContent id={id} />
    </Suspense>
  );
}

interface BookContentProps {
  id: string;
}

function BookContent({ id }: BookContentProps): ReactElement {
  const { data: book } = useSuspenseQuery({
    queryKey: queryKeys.books.detail(id),
    queryFn: ({ signal }) => getBook(id, signal),
  });

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
          <div className="border-border bg-surface-1 relative aspect-[2/3] overflow-hidden rounded-md border">
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
                  viewTransition
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

          <Tabs defaultValue="overview" className="w-full">
            <TabsList>
              <TabsTrigger value="overview">Overview</TabsTrigger>
              <TabsTrigger value="versions">
                Versions
                {book.metadata_version_summary.pending > 0 ? (
                  <Badge variant="secondary" className="ml-2">
                    {book.metadata_version_summary.pending}
                  </Badge>
                ) : null}
              </TabsTrigger>
              <TabsTrigger value="activity">Activity</TabsTrigger>
            </TabsList>

            <TabsContent value="overview">
              <OverviewTab book={book} />
            </TabsContent>
            <TabsContent value="versions">
              <VersionsTab book={book} />
            </TabsContent>
            <TabsContent value="activity">
              <p className="text-fg-muted text-sm">Activity timeline lands in a later sub-phase.</p>
            </TabsContent>
          </Tabs>
        </section>
      </div>
    </div>
  );
}

interface DetailCoverProps {
  book: BookDetail;
}

/**
 * Real cover art with the typographic spine fallback (spec S8) — the
 * detail page mirrors the grid tile so a missing or failing cover
 * never renders the browser's broken-image glyph.
 */
function DetailCover({ book }: DetailCoverProps): ReactElement {
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

interface TabProps {
  book: BookDetail;
}

function OverviewTab({ book }: TabProps): ReactElement {
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

interface FieldProps {
  label: string;
  value: string;
  mono?: boolean;
}

function Field({ label, value, mono = false }: FieldProps): ReactElement {
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
