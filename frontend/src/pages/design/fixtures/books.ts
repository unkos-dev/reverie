/**
 * Design-system hero-screen fixtures.
 *
 * Real-feeling library data for the D4 hero screens (`/design/hero/library`,
 * `/design/hero/book`). Mixed standalone + series; mixed short and long
 * titles; literary fiction, classics, non-fiction, poetry — enough variety
 * that the grid reads as a curated library rather than a fixtures dump.
 *
 * This file lives under `src/pages/design/` so it is included in the
 * dev-only `design` chunk and dead-stripped from the production bundle
 * by the Vite `manualChunks` rule in `frontend/vite.config.ts`.
 *
 * Field shape mirrors the eventual REST API shape from Step 11 closely
 * enough that the hero screens prototype real component contracts, but
 * the fixtures are not load-bearing — Step 11 is free to refine the
 * shape and migrate the hero pages with it.
 */

/**
 * Book record consumed by the D4 hero screens.
 *
 * Field shape mirrors the eventual REST API shape from Step 11 closely
 * enough that hero pages prototype real component contracts. Optional
 * fields (`series`, `pageCount`, `description`, `shelves`, `readingState`)
 * are absent on books for which the metadata is not yet curated.
 *
 * `formats` is a closed set of `"EPUB" | "PDF" | "MOBI"` — extending it
 * is a breaking change for the cover-format Badges in `book.tsx`.
 */
export interface Book {
  id: string;
  title: string;
  authors: string[];
  series?: {
    name: string;
    position: number;
  };
  publishedYear: number;
  publisher: string;
  language: string;
  isbn13: string;
  pageCount?: number;
  formats: ("EPUB" | "PDF" | "MOBI")[];
  shelves?: string[];
  description?: string;
  readingState?: "unread" | "reading" | "finished";
}

/**
 * Canonical fixture array for the D4 hero screens.
 *
 * Non-empty tuple type (`[Book, ...Book[]]`) so consumers can rely on
 * `BOOKS[0]` without a runtime guard. Book `id` values are unique within
 * this array; downstream lookups via {@link bookById} assume uniqueness.
 */
export const BOOKS: [Book, ...Book[]] = [
  {
    id: "brothers-karamazov",
    title: "The Brothers Karamazov",
    authors: ["Fyodor Dostoevsky"],
    publishedYear: 1880,
    publisher: "The Russian Messenger",
    language: "English",
    isbn13: "9780374528379",
    pageCount: 796,
    formats: ["EPUB", "PDF"],
    shelves: ["Russian classics", "To revisit"],
    readingState: "finished",
    description:
      "A passionate, polyphonic novel of patricide, faith and free will. Three brothers — sensual Dmitri, sceptical Ivan, gentle Alyosha — orbit a father whose murder forces each to reckon with the God he loves, the God he doubts, and the God he hates.",
  },
  {
    id: "wind-up-bird",
    title: "The Wind-Up Bird Chronicle",
    authors: ["Haruki Murakami"],
    publishedYear: 1994,
    publisher: "Shinchosha",
    language: "English",
    isbn13: "9780679775430",
    pageCount: 607,
    formats: ["EPUB"],
    shelves: ["Japanese fiction"],
    readingState: "reading",
    description:
      "Toru Okada's wife disappears and so does his cat; in the slow unravelling that follows, a dry well, a war veteran, and a mark on his cheek pull him into a Tokyo that nobody around him can quite see.",
  },
  {
    id: "name-of-the-rose",
    title: "The Name of the Rose",
    authors: ["Umberto Eco"],
    publishedYear: 1980,
    publisher: "Bompiani",
    language: "English",
    isbn13: "9780156001311",
    pageCount: 502,
    formats: ["EPUB"],
    shelves: ["Mystery", "Medieval"],
    readingState: "finished",
  },
  {
    id: "remains-of-the-day",
    title: "The Remains of the Day",
    authors: ["Kazuo Ishiguro"],
    publishedYear: 1989,
    publisher: "Faber & Faber",
    language: "English",
    isbn13: "9780571258246",
    pageCount: 258,
    formats: ["EPUB"],
    shelves: ["Booker winners"],
    readingState: "unread",
  },
  {
    id: "earthsea-1",
    title: "A Wizard of Earthsea",
    authors: ["Ursula K. Le Guin"],
    series: { name: "Earthsea Cycle", position: 1 },
    publishedYear: 1968,
    publisher: "Parnassus Press",
    language: "English",
    isbn13: "9780547851402",
    pageCount: 205,
    formats: ["EPUB"],
    shelves: ["Fantasy"],
    readingState: "finished",
  },
  {
    id: "earthsea-2",
    title: "The Tombs of Atuan",
    authors: ["Ursula K. Le Guin"],
    series: { name: "Earthsea Cycle", position: 2 },
    publishedYear: 1971,
    publisher: "Atheneum",
    language: "English",
    isbn13: "9780689845369",
    pageCount: 192,
    formats: ["EPUB"],
    shelves: ["Fantasy"],
    readingState: "finished",
  },
  {
    id: "earthsea-3",
    title: "The Farthest Shore",
    authors: ["Ursula K. Le Guin"],
    series: { name: "Earthsea Cycle", position: 3 },
    publishedYear: 1972,
    publisher: "Atheneum",
    language: "English",
    isbn13: "9780689845346",
    pageCount: 223,
    formats: ["EPUB"],
    shelves: ["Fantasy"],
    readingState: "reading",
  },
  {
    id: "earthsea-4",
    title: "Tehanu",
    authors: ["Ursula K. Le Guin"],
    series: { name: "Earthsea Cycle", position: 4 },
    publishedYear: 1990,
    publisher: "Atheneum",
    language: "English",
    isbn13: "9780689845338",
    pageCount: 252,
    formats: ["EPUB"],
    shelves: ["Fantasy"],
    readingState: "unread",
  },
  {
    id: "stoner",
    title: "Stoner",
    authors: ["John Williams"],
    publishedYear: 1965,
    publisher: "Viking Press",
    language: "English",
    isbn13: "9781590171998",
    pageCount: 278,
    formats: ["EPUB"],
    shelves: ["Quiet novels"],
    readingState: "finished",
  },
  {
    id: "thinking-fast-and-slow",
    title: "Thinking, Fast and Slow",
    authors: ["Daniel Kahneman"],
    publishedYear: 2011,
    publisher: "Farrar, Straus and Giroux",
    language: "English",
    isbn13: "9780374533557",
    pageCount: 499,
    formats: ["EPUB", "PDF"],
    shelves: ["Non-fiction"],
    readingState: "reading",
  },
  {
    id: "annihilation",
    title: "Annihilation",
    authors: ["Jeff VanderMeer"],
    series: { name: "Southern Reach", position: 1 },
    publishedYear: 2014,
    publisher: "FSG Originals",
    language: "English",
    isbn13: "9780374104092",
    pageCount: 195,
    formats: ["EPUB"],
    shelves: ["Weird fiction"],
    readingState: "finished",
  },
  {
    id: "authority",
    title: "Authority",
    authors: ["Jeff VanderMeer"],
    series: { name: "Southern Reach", position: 2 },
    publishedYear: 2014,
    publisher: "FSG Originals",
    language: "English",
    isbn13: "9780374104108",
    pageCount: 341,
    formats: ["EPUB"],
    shelves: ["Weird fiction"],
    readingState: "unread",
  },
  {
    id: "acceptance",
    title: "Acceptance",
    authors: ["Jeff VanderMeer"],
    series: { name: "Southern Reach", position: 3 },
    publishedYear: 2014,
    publisher: "FSG Originals",
    language: "English",
    isbn13: "9780374104115",
    pageCount: 341,
    formats: ["EPUB"],
    shelves: ["Weird fiction"],
    readingState: "unread",
  },
  {
    id: "piranesi",
    title: "Piranesi",
    authors: ["Susanna Clarke"],
    publishedYear: 2020,
    publisher: "Bloomsbury",
    language: "English",
    isbn13: "9781635575637",
    pageCount: 245,
    formats: ["EPUB"],
    shelves: ["Quiet novels", "Weird fiction"],
    readingState: "finished",
  },
  {
    id: "code-of-the-woosters",
    title: "The Code of the Woosters",
    authors: ["P.G. Wodehouse"],
    series: { name: "Jeeves", position: 7 },
    publishedYear: 1938,
    publisher: "Herbert Jenkins",
    language: "English",
    isbn13: "9780393339819",
    pageCount: 252,
    formats: ["EPUB"],
    shelves: ["Comfort"],
    readingState: "finished",
  },
  {
    id: "left-hand-of-darkness",
    title: "The Left Hand of Darkness",
    authors: ["Ursula K. Le Guin"],
    series: { name: "Hainish Cycle", position: 4 },
    publishedYear: 1969,
    publisher: "Ace Books",
    language: "English",
    isbn13: "9780441478125",
    pageCount: 286,
    formats: ["EPUB"],
    shelves: ["Science fiction"],
    readingState: "finished",
  },
  {
    id: "blood-meridian",
    title: "Blood Meridian, Or the Evening Redness in the West",
    authors: ["Cormac McCarthy"],
    publishedYear: 1985,
    publisher: "Random House",
    language: "English",
    isbn13: "9780679728757",
    pageCount: 337,
    formats: ["EPUB"],
    shelves: ["American gothic"],
    readingState: "unread",
  },
  {
    id: "tao",
    title: "Tao Te Ching",
    authors: ["Lao Tzu", "Stephen Mitchell (tr.)"],
    publishedYear: -400,
    publisher: "Harper Perennial",
    language: "English",
    isbn13: "9780061142666",
    pageCount: 128,
    formats: ["EPUB"],
    shelves: ["Philosophy"],
    readingState: "finished",
  },
  {
    id: "convenience-store-woman",
    title: "Convenience Store Woman",
    authors: ["Sayaka Murata"],
    publishedYear: 2016,
    publisher: "Bungeishunjū",
    language: "English",
    isbn13: "9780802128256",
    pageCount: 163,
    formats: ["EPUB"],
    shelves: ["Japanese fiction"],
    readingState: "finished",
  },
  {
    id: "klara-and-the-sun",
    title: "Klara and the Sun",
    authors: ["Kazuo Ishiguro"],
    publishedYear: 2021,
    publisher: "Faber & Faber",
    language: "English",
    isbn13: "9780571364879",
    pageCount: 320,
    formats: ["EPUB"],
    shelves: ["Science fiction"],
    readingState: "reading",
  },
  {
    id: "pale-fire",
    title: "Pale Fire",
    authors: ["Vladimir Nabokov"],
    publishedYear: 1962,
    publisher: "G.P. Putnam's Sons",
    language: "English",
    isbn13: "9780679723424",
    pageCount: 246,
    formats: ["EPUB"],
    shelves: ["Difficult & rewarding"],
    readingState: "finished",
  },
  {
    id: "selected-poems-bishop",
    title: "Selected Poems",
    authors: ["Elizabeth Bishop"],
    publishedYear: 1991,
    publisher: "Farrar, Straus and Giroux",
    language: "English",
    isbn13: "9780374531898",
    pageCount: 128,
    formats: ["EPUB"],
    shelves: ["Poetry"],
    readingState: "reading",
  },
  {
    id: "long-way-to-a-small-angry-planet",
    title: "The Long Way to a Small, Angry Planet",
    authors: ["Becky Chambers"],
    series: { name: "Wayfarers", position: 1 },
    publishedYear: 2014,
    publisher: "Hodder & Stoughton",
    language: "English",
    isbn13: "9781473619814",
    pageCount: 404,
    formats: ["EPUB"],
    shelves: ["Comfort", "Science fiction"],
    readingState: "finished",
  },
  {
    id: "in-the-distance",
    title: "In the Distance",
    authors: ["Hernan Diaz"],
    publishedYear: 2017,
    publisher: "Coffee House Press",
    language: "English",
    isbn13: "9781566894883",
    pageCount: 272,
    formats: ["EPUB"],
    shelves: ["American gothic"],
    readingState: "unread",
  },
];

/**
 * Look up a {@link Book} by its `id`. Returns `undefined` when no fixture
 * matches — callers are expected to handle the missing case (the hero
 * book page falls back to {@link FEATURED_BOOK_ID}).
 */
export function bookById(id: string): Book | undefined {
  return BOOKS.find((b) => b.id === id);
}

/**
 * Stable id of the book rendered as the hero default when the book-detail
 * route is visited without an `?id=` param or with an unmatched id.
 * Must be present in {@link BOOKS}.
 */
export const FEATURED_BOOK_ID = "wind-up-bird";

/**
 * Reading-state shelves derived from {@link BOOKS}, in display order.
 *
 * Counts are computed once at module load — fixture data is static, so
 * a recompute is unnecessary. Order is load-bearing for the hero page:
 * the "Currently reading" entry is referenced by name (not index) in
 * the library page header.
 */
export const SHELVES: { name: string; count: number }[] = [
  {
    name: "Currently reading",
    count: BOOKS.filter((b) => b.readingState === "reading").length,
  },
  {
    name: "Want to read",
    count: BOOKS.filter((b) => b.readingState === "unread").length,
  },
  {
    name: "Finished",
    count: BOOKS.filter((b) => b.readingState === "finished").length,
  },
];
