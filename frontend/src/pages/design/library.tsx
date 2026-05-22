import type { ReactElement } from "react";
import { useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router";
import { BookOpen, LayoutGrid, List, Search, SlidersHorizontal } from "lucide-react";
import { Lockup } from "@/components/Lockup";
import { ThemeSwitcher } from "@/components/theme-switcher";
import { CoverArtwork } from "./components/CoverArtwork";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { useTheme } from "@/lib/theme/ThemeProvider";
import { BOOKS, SHELVES, type Book } from "./fixtures/books";

type ViewMode = "grid" | "list";

// Per philosophy state table, "Selected" maps to `bg-accent-soft +
// text-fg`; shadcn's Button variants don't ship a selected mode, so
// overlay the brand mapping on the ghost variant when aria-pressed.
function selectedClass(active: boolean): string {
  return active ? "bg-accent-soft text-fg hover:bg-accent-soft" : "";
}

interface BookCardProps {
  book: Book;
}

function BookCard({ book }: BookCardProps): ReactElement {
  const seriesLabel = book.series ? `${book.series.name} · #${String(book.series.position)}` : null;

  // Whole card is one link target — cover + title both navigate to the
  // detail route, matching BookRow's contract and the visual hover-lift
  // affordance on the article wrapper.
  return (
    <article className="group">
      <Link
        to={`/design/hero/book?id=${book.id}`}
        className="flex flex-col gap-3 rounded-md transition-transform duration-200 hover:-translate-y-0.5 focus-visible:outline-none focus-visible:-translate-y-0.5 focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-canvas"
      >
        <div className="relative overflow-hidden rounded-md border border-border group-hover:border-border-strong transition-colors">
          <CoverArtwork bookId={book.id} title={book.title} authors={book.authors} />
          {seriesLabel ? (
            <span className="absolute top-2 left-2 bg-canvas/85 text-fg text-[0.62rem] uppercase tracking-[0.14em] px-2 py-1 rounded-sm border border-border backdrop-blur-sm">
              {seriesLabel}
            </span>
          ) : null}
          {book.readingState === "reading" ? (
            <span className="absolute bottom-2 right-2 bg-accent-soft text-fg text-[0.62rem] uppercase tracking-[0.14em] px-2 py-1 rounded-sm font-semibold">
              Reading
            </span>
          ) : null}
        </div>
        <div className="flex flex-col gap-1">
          <h3 className="font-display font-semibold text-fg text-sm leading-tight line-clamp-2">
            {book.title}
          </h3>
          <p className="text-fg-muted text-xs leading-tight line-clamp-1">
            {book.authors.join(", ")}
          </p>
        </div>
      </Link>
    </article>
  );
}

interface BookRowProps {
  book: Book;
}

function BookRow({ book }: BookRowProps): ReactElement {
  return (
    <Link
      to={`/design/hero/book?id=${book.id}`}
      className="flex gap-4 items-center px-3 py-3 -mx-3 rounded-md border border-transparent hover:bg-surface-2 hover:border-border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-canvas transition-colors"
    >
      <div className="w-12 shrink-0">
        <CoverArtwork bookId={book.id} title={book.title} authors={book.authors} />
      </div>
      <div className="min-w-0 flex-1">
        <h3 className="font-display font-semibold text-fg text-sm leading-tight truncate">
          {book.title}
        </h3>
        <p className="text-fg-muted text-xs truncate">
          {book.authors.join(", ")}
          {book.series ? (
            <span className="text-fg-faint">
              {" · "}
              {book.series.name} #{String(book.series.position)}
            </span>
          ) : null}
        </p>
      </div>
      <div className="hidden md:flex shrink-0 items-center gap-3 text-xs text-fg-muted">
        <span>{String(book.publishedYear)}</span>
        <span className="font-mono">{book.formats.join(" · ")}</span>
        {book.readingState === "reading" ? (
          <Badge className="bg-accent-soft text-fg border-transparent">Reading</Badge>
        ) : null}
      </div>
    </Link>
  );
}

export default function HeroLibraryPage(): ReactElement {
  const { effective } = useTheme();
  const [searchParams, setSearchParams] = useSearchParams();
  const [viewMode, setViewMode] = useState<ViewMode>("grid");
  const [query, setQuery] = useState("");
  const [activeShelf, setActiveShelf] = useState<string | null>(null);

  // `?series=<name>` (set by the book-detail series link) drives a
  // dedicated series filter alongside the shelf chips. Series and
  // shelves overlap conceptually but are distinct fields in the
  // fixture; treat them as independent filters that AND together.
  const activeSeries = searchParams.get("series");

  function clearSeriesFilter(): void {
    const next = new URLSearchParams(searchParams);
    next.delete("series");
    setSearchParams(next, { replace: true });
  }

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return BOOKS.filter((book) => {
      if (activeShelf && !(book.shelves?.includes(activeShelf) ?? false)) {
        return false;
      }
      if (activeSeries && book.series?.name !== activeSeries) {
        return false;
      }
      if (q.length === 0) return true;
      return (
        book.title.toLowerCase().includes(q) ||
        book.authors.some((a) => a.toLowerCase().includes(q)) ||
        (book.series?.name.toLowerCase().includes(q) ?? false)
      );
    });
  }, [query, activeShelf, activeSeries]);

  const allShelves = useMemo(() => {
    const set = new Set<string>();
    for (const book of BOOKS) {
      for (const shelf of book.shelves ?? []) {
        set.add(shelf);
      }
    }
    return Array.from(set).sort();
  }, []);

  return (
    <main className="bg-canvas text-fg min-h-screen">
      <header className="bg-canvas/90 supports-[backdrop-filter]:backdrop-blur-md border-b border-border sticky top-0 z-20">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-3 flex items-center gap-4">
          <Lockup size={24} theme={effective === "dark" ? "dark" : "light"} />
          <span className="text-fg-faint text-xs hidden sm:inline">/ library</span>
          <div className="flex-1" />
          <div className="hidden md:block relative w-72 max-w-full">
            <Search
              className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-fg-muted pointer-events-none"
              aria-hidden="true"
            />
            <Input
              type="search"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
              }}
              placeholder="Search the library"
              className="pl-9"
              aria-label="Search the library"
            />
          </div>
          <ThemeSwitcher />
        </div>
      </header>

      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <section className="flex flex-col gap-2 mb-8">
          <p className="text-fg-faint text-xs uppercase tracking-[0.22em]">Your library</p>
          <h1 className="font-display text-3xl sm:text-4xl lg:text-5xl font-semibold leading-[1.05] text-fg">
            A library worth keeping.
          </h1>
          <p className="text-fg-muted max-w-xl">
            {String(BOOKS.length)} works · {String(SHELVES[0]?.count ?? 0)} in progress · last
            imported 2 hours ago
          </p>
        </section>

        <div className="md:hidden mb-4 relative">
          <Search
            className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-fg-muted pointer-events-none"
            aria-hidden="true"
          />
          <Input
            type="search"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
            }}
            placeholder="Search the library"
            className="pl-9"
            aria-label="Search the library"
          />
        </div>

        {activeSeries ? (
          <div className="mb-4 flex flex-wrap items-center gap-2 text-sm">
            <span className="text-fg-faint text-xs uppercase tracking-[0.16em]">Series filter</span>
            <span className="font-display font-semibold text-fg">{activeSeries}</span>
            <Button variant="ghost" size="sm" onClick={clearSeriesFilter}>
              Clear
            </Button>
          </div>
        ) : null}

        <div className="flex flex-wrap gap-2 items-center mb-6">
          <Button
            variant="ghost"
            size="sm"
            aria-pressed={activeShelf === null}
            className={selectedClass(activeShelf === null)}
            onClick={() => {
              setActiveShelf(null);
            }}
          >
            All
          </Button>
          {allShelves.map((shelf) => (
            <Button
              key={shelf}
              variant="ghost"
              size="sm"
              aria-pressed={activeShelf === shelf}
              className={selectedClass(activeShelf === shelf)}
              onClick={() => {
                setActiveShelf(shelf);
              }}
            >
              {shelf}
            </Button>
          ))}
          <div className="flex-1" />
          <Button variant="ghost" size="sm" aria-label="Sort and filter">
            <SlidersHorizontal className="w-4 h-4" aria-hidden="true" />
            <span className="hidden sm:inline">Sort</span>
          </Button>
          <div className="flex h-6 items-center">
            <Separator orientation="vertical" />
          </div>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setViewMode("grid");
            }}
            aria-label="Grid view"
            aria-pressed={viewMode === "grid"}
            className={selectedClass(viewMode === "grid")}
          >
            <LayoutGrid className="w-4 h-4" aria-hidden="true" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setViewMode("list");
            }}
            aria-label="List view"
            aria-pressed={viewMode === "list"}
            className={selectedClass(viewMode === "list")}
          >
            <List className="w-4 h-4" aria-hidden="true" />
          </Button>
        </div>

        <h2 className="sr-only">Books</h2>
        {filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-24 text-center gap-3 border border-dashed border-border rounded-md">
            <BookOpen className="w-8 h-8 text-fg-muted" aria-hidden="true" />
            <p className="font-display text-lg text-fg">Nothing on that shelf.</p>
            <p className="text-fg-muted text-sm max-w-sm">
              Clear the filter or try a different search term.
            </p>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setQuery("");
                setActiveShelf(null);
                if (activeSeries) clearSeriesFilter();
              }}
            >
              Clear filters
            </Button>
          </div>
        ) : viewMode === "grid" ? (
          <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-x-5 gap-y-8">
            {filtered.map((book) => (
              <BookCard key={book.id} book={book} />
            ))}
          </div>
        ) : (
          <div className="flex flex-col">
            {filtered.map((book, idx) => (
              <div key={book.id}>
                <BookRow book={book} />
                {idx < filtered.length - 1 ? <Separator className="opacity-60" /> : null}
              </div>
            ))}
          </div>
        )}
      </div>
    </main>
  );
}
