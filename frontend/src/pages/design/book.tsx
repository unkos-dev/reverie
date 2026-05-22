import type { ReactElement, ReactNode } from "react";
import { Link, useSearchParams } from "react-router";
import {
  ArrowLeft,
  BookOpen,
  Bookmark,
  ChevronRight,
  Download,
  History,
  Pencil,
} from "lucide-react";
import { Lockup } from "@/components/Lockup";
import { ThemeSwitcher } from "@/components/theme-switcher";
import { CoverArtwork } from "./components/CoverArtwork";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useTheme } from "@/lib/theme/ThemeProvider";
import { bookById, BOOKS, FEATURED_BOOK_ID, type Book } from "./fixtures/books";

function resolveBook(idParam: string | null): Book {
  if (idParam) {
    const match = bookById(idParam);
    if (match) return match;
  }
  const featured = bookById(FEATURED_BOOK_ID);
  return featured ?? BOOKS[0];
}

interface MetadataRowProps {
  label: string;
  children: ReactNode;
}

function MetadataRow({ label, children }: MetadataRowProps): ReactElement {
  return (
    <div className="grid grid-cols-[8rem_1fr] gap-3 py-2.5 border-b border-border last:border-b-0">
      <dt className="text-fg-faint text-xs uppercase tracking-[0.16em] pt-0.5">{label}</dt>
      <dd className="text-fg text-sm">{children}</dd>
    </div>
  );
}

interface VersionRowProps {
  version: string;
  source: string;
  status: "accepted" | "draft" | "rejected";
  timestamp: string;
}

function VersionRow({ version, source, status, timestamp }: VersionRowProps): ReactElement {
  return (
    <div className="flex items-start gap-4 py-3 border-b border-border last:border-b-0">
      <div className="flex flex-col items-center pt-1">
        <History className="w-4 h-4 text-fg-muted" aria-hidden="true" />
      </div>
      <div className="flex-1 min-w-0">
        <div className="flex flex-wrap items-baseline gap-2">
          <span className="font-display font-semibold text-fg">{version}</span>
          <span className="text-fg-muted text-xs">{source}</span>
        </div>
        <p className="text-fg-faint text-xs mt-0.5">{timestamp}</p>
      </div>
      <Badge
        variant="outline"
        className={status === "accepted" ? "font-semibold text-fg" : "text-fg-muted"}
      >
        {status}
      </Badge>
    </div>
  );
}

export default function HeroBookPage(): ReactElement {
  const { effective } = useTheme();
  const [searchParams] = useSearchParams();
  const book = resolveBook(searchParams.get("id"));

  return (
    <main className="bg-canvas text-fg min-h-screen">
      <header className="bg-canvas/90 supports-[backdrop-filter]:backdrop-blur-md border-b border-border sticky top-0 z-20">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-3 flex items-center gap-4">
          <Lockup size={24} theme={effective === "dark" ? "dark" : "light"} />
          <nav aria-label="Breadcrumb" className="hidden sm:flex items-center gap-1 text-xs">
            <Link
              to="/design/hero/library"
              className="text-fg-muted hover:text-fg rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-canvas px-1"
            >
              library
            </Link>
            <ChevronRight className="w-3.5 h-3.5 text-fg-faint" aria-hidden="true" />
            <span className="text-fg-faint truncate max-w-[16rem]">{book.title}</span>
          </nav>
          <div className="flex-1" />
          <Button
            variant="ghost"
            size="sm"
            asChild
            className="sm:hidden"
            aria-label="Back to library"
          >
            <Link to="/design/hero/library">
              <ArrowLeft className="w-4 h-4" aria-hidden="true" />
            </Link>
          </Button>
          <ThemeSwitcher />
        </div>
      </header>

      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8 lg:py-12">
        <div className="grid grid-cols-1 lg:grid-cols-[18rem_1fr] xl:grid-cols-[22rem_1fr] gap-8 lg:gap-12">
          <aside className="flex flex-col gap-4 lg:sticky lg:top-24 lg:self-start">
            <div className="mx-auto w-40 sm:w-56 lg:w-full rounded-md overflow-hidden border border-border shadow-sm">
              <CoverArtwork bookId={book.id} title={book.title} authors={book.authors} />
            </div>
            <div className="flex flex-col gap-2">
              <Button size="lg" className="w-full">
                <BookOpen className="w-4 h-4" aria-hidden="true" />
                Read
              </Button>
              <div className="grid grid-cols-3 gap-2">
                <Button variant="outline" size="sm" aria-label="Download">
                  <Download className="w-4 h-4" aria-hidden="true" />
                  <span className="hidden sm:inline">EPUB</span>
                </Button>
                <Button variant="outline" size="sm" aria-label="Edit metadata">
                  <Pencil className="w-4 h-4" aria-hidden="true" />
                  <span className="hidden sm:inline">Edit</span>
                </Button>
                <Button variant="outline" size="sm" aria-label="Add to shelf">
                  <Bookmark className="w-4 h-4" aria-hidden="true" />
                  <span className="hidden sm:inline">Shelve</span>
                </Button>
              </div>
            </div>
          </aside>

          <article className="flex flex-col gap-6 min-w-0">
            <header className="flex flex-col gap-2">
              {book.series ? (
                <p className="text-fg-faint text-xs uppercase tracking-[0.22em]">
                  <Link
                    to={`/design/hero/library?series=${encodeURIComponent(book.series.name)}`}
                    className="hover:text-fg rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-canvas"
                  >
                    {book.series.name}
                  </Link>
                  <span className="text-fg-faint">
                    {" · book "}
                    {String(book.series.position)}
                  </span>
                </p>
              ) : null}
              <h1 className="font-display font-semibold text-fg text-3xl sm:text-4xl lg:text-5xl leading-[1.05] tracking-tight">
                {book.title}
              </h1>
              <p className="font-body text-fg-muted text-lg">{book.authors.join(", ")}</p>
              <div className="flex flex-wrap items-center gap-2 mt-2">
                {book.readingState === "reading" ? (
                  <Badge className="bg-accent-soft text-fg border-transparent">
                    Currently reading
                  </Badge>
                ) : null}
                {book.readingState === "finished" ? (
                  <Badge variant="outline">Finished</Badge>
                ) : null}
                {book.shelves?.map((shelf) => (
                  <Badge key={shelf} variant="outline">
                    {shelf}
                  </Badge>
                ))}
              </div>
            </header>

            <Separator />

            <Tabs defaultValue="overview" className="gap-6">
              <TabsList>
                <TabsTrigger value="overview">Overview</TabsTrigger>
                <TabsTrigger value="versions">Versions</TabsTrigger>
                <TabsTrigger value="activity">Activity</TabsTrigger>
              </TabsList>

              <TabsContent value="overview" className="flex flex-col gap-8">
                {book.description ? (
                  <section>
                    <h2 className="sr-only">Description</h2>
                    <p className="font-body text-fg text-base leading-relaxed">
                      {book.description}
                    </p>
                  </section>
                ) : (
                  <p className="font-body text-fg-muted italic">
                    No description recorded. Enrichment will fill this in on the next pass.
                  </p>
                )}

                <section>
                  <h2 className="font-display text-fg text-sm uppercase tracking-[0.18em] mb-2">
                    Details
                  </h2>
                  <dl>
                    <MetadataRow label="Published">
                      {String(book.publishedYear)} · {book.publisher}
                    </MetadataRow>
                    <MetadataRow label="Language">{book.language}</MetadataRow>
                    <MetadataRow label="ISBN">
                      <span className="font-mono">{book.isbn13}</span>
                    </MetadataRow>
                    {book.pageCount !== undefined ? (
                      <MetadataRow label="Length">{String(book.pageCount)} pages</MetadataRow>
                    ) : null}
                    <MetadataRow label="Formats">
                      <span className="flex flex-wrap gap-1.5">
                        {book.formats.map((fmt) => (
                          <Badge key={fmt} variant="outline">
                            {fmt}
                          </Badge>
                        ))}
                      </span>
                    </MetadataRow>
                  </dl>
                </section>
              </TabsContent>

              <TabsContent value="versions" className="flex flex-col gap-4">
                <h2 className="sr-only">Metadata versions</h2>
                <p className="text-fg-muted text-sm">
                  Reverie tracks each metadata revision. Accept a draft to make it canonical;
                  rejected versions stay in history.
                </p>
                <div>
                  <VersionRow
                    version="v3 · current"
                    source="Open Library · enrichment"
                    status="accepted"
                    timestamp="Accepted 2 days ago"
                  />
                  <VersionRow
                    version="v2"
                    source="Google Books · enrichment"
                    status="draft"
                    timestamp="Suggested 2 days ago"
                  />
                  <VersionRow
                    version="v1"
                    source="EPUB OPF · ingestion"
                    status="rejected"
                    timestamp="Captured on import"
                  />
                </div>
              </TabsContent>

              <TabsContent value="activity" className="flex flex-col gap-4">
                <h2 className="sr-only">Reading activity</h2>
                <p className="text-fg-muted text-sm">
                  Reading activity is captured by sync adapters; none are wired to this fixture yet.
                </p>
                <div className="border border-dashed border-border rounded-md p-8 flex flex-col items-center justify-center gap-2 text-center">
                  <History className="w-6 h-6 text-fg-muted" aria-hidden="true" />
                  <p className="text-fg text-sm font-display">No reading activity yet.</p>
                  <p className="text-fg-muted text-xs max-w-sm">
                    Once a Kobo, KOReader or OPDS-Progression sync is connected, progress and notes
                    will land here.
                  </p>
                </div>
              </TabsContent>
            </Tabs>
          </article>
        </div>
      </div>
    </main>
  );
}
