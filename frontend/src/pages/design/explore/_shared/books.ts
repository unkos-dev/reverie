export type Book = {
  id: string;
  title: string;
  author: string;
  year: number;
  status: "unread" | "in-progress" | "finished";
  progress?: number;
  format: "epub" | "pdf";
  pages: number;
  addedDays: number;
  series?: { name: string; index: number; total: number };
  genres?: string[];
  lastReadDays?: number;
  language?: string;
  isbn?: string;
  ratings?: { google?: number; hardcover?: number; goodreads?: number };
};

export const BOOKS: Book[] = [
  { id: "b01", title: "The Glass Bell",            author: "Anouk Verheven",     year: 2021, status: "in-progress", progress: 0.42, format: "epub", pages: 312, addedDays: 4,   genres: ["Literary", "Mystery"],         lastReadDays: 1,   language: "en", isbn: "9781784878322", ratings: { google: 4.4, hardcover: 4.1, goodreads: 4.0 } },
  { id: "b02", title: "Salt and Cipher",           author: "Idris Bekele",       year: 2019, status: "in-progress", progress: 0.78, format: "epub", pages: 488, addedDays: 11,  genres: ["Literary"],                    lastReadDays: 2,   language: "en", isbn: "9781250214829", ratings: { google: 4.6, hardcover: 4.4, goodreads: 4.3 } },
  { id: "b03", title: "The Kept Garden",           author: "Mercer Hadley",      year: 2023, status: "finished",                    format: "epub", pages: 264, addedDays: 19,  genres: ["Memoir", "Nature"],            lastReadDays: 14,  language: "en", isbn: "9780571362936", ratings: { google: 4.2, hardcover: 4.0, goodreads: 3.9 } },
  { id: "b04", title: "Lantern Pulse",             author: "Yusra Al-Mansouri",  year: 2022, status: "unread",                      format: "epub", pages: 401, addedDays: 2,   series: { name: "Lantern Cycle", index: 1, total: 4 }, genres: ["Sci-fi", "Adventure"],         language: "en", isbn: "9780765395344", ratings: { google: 4.5, hardcover: 4.5, goodreads: 4.4 } },
  { id: "b05", title: "Quiet Continent",           author: "Tomás Fialho",       year: 2018, status: "finished",                    format: "epub", pages: 552, addedDays: 88,  genres: ["History", "Travel"],           lastReadDays: 60,  language: "pt", isbn: "9789895624072", ratings: { google: 4.0, hardcover: 3.9, goodreads: 3.8 } },
  { id: "b06", title: "Architecture of the Void",  author: "Saskia Brandt",      year: 2024, status: "unread",                      format: "pdf",  pages: 196, addedDays: 1,   genres: ["Architecture", "Essay"],                          language: "de", isbn: "9783863353018", ratings: { google: 4.3, hardcover: 4.2, goodreads: 4.1 } },
  { id: "b07", title: "The Cartographer's Daughter", author: "June Okonkwo",     year: 2020, status: "in-progress", progress: 0.16, format: "epub", pages: 368, addedDays: 27,  genres: ["Literary", "Family"],          lastReadDays: 5,   language: "en", isbn: "9781250271396", ratings: { google: 4.4, hardcover: 4.3, goodreads: 4.2 } },
  { id: "b08", title: "Riverlight Months",         author: "Hadrien Cosse",      year: 2017, status: "finished",                    format: "epub", pages: 224, addedDays: 142, genres: ["Poetry"],                      lastReadDays: 110, language: "fr", isbn: "9782070451462", ratings: { google: 4.7, hardcover: 4.5, goodreads: 4.5 } },
  { id: "b09", title: "Notes on Disappearance",    author: "Petra Wells",        year: 2023, status: "unread",                      format: "epub", pages: 308, addedDays: 5,   genres: ["Memoir", "Philosophy"],                           language: "en", isbn: "9780374605865", ratings: { google: 4.1, hardcover: 4.0, goodreads: 3.9 } },
  { id: "b10", title: "The Long Tide",             author: "Oluwaseun Kemi",     year: 2022, status: "unread",                      format: "epub", pages: 416, addedDays: 9,   genres: ["Historical Fiction"],                             language: "en", isbn: "9780063140189", ratings: { google: 4.3, hardcover: 4.2, goodreads: 4.0 } },
  { id: "b11", title: "House at Six Hills",        author: "Ines Roca",          year: 2015, status: "finished",                    format: "epub", pages: 188, addedDays: 410, genres: ["Literary"],                    lastReadDays: 380, language: "es", isbn: "9788433925022", ratings: { google: 4.4, hardcover: 4.3, goodreads: 4.1 } },
  { id: "b12", title: "Inland",                    author: "Mateusz Borowicz",   year: 2021, status: "unread",                      format: "epub", pages: 272, addedDays: 14,  genres: ["Literary", "Translation"],                        language: "pl", isbn: "9788381911900", ratings: { google: 4.0, hardcover: 4.1, goodreads: 3.8 } },
  { id: "b13", title: "Atlas of Familiar Things",  author: "Naia Linde",         year: 2024, status: "unread",                      format: "epub", pages: 344, addedDays: 3,   genres: ["Essay", "Nature"],                                language: "en", isbn: "9780571381623", ratings: { google: 4.5, hardcover: 4.4, goodreads: 4.3 } },
  { id: "b14", title: "What Remains After Sleep",  author: "August Marlow",      year: 2020, status: "finished",                    format: "epub", pages: 296, addedDays: 220, genres: ["Literary", "Speculative"],     lastReadDays: 200, language: "en", isbn: "9781250756312", ratings: { google: 4.2, hardcover: 4.1, goodreads: 4.0 } },
  { id: "b15", title: "Three Empty Rooms",         author: "Lila Vasquez",       year: 2019, status: "unread",                      format: "epub", pages: 232, addedDays: 36,  genres: ["Literary"],                                       language: "en", isbn: "9780525521792", ratings: { google: 3.9, hardcover: 3.8, goodreads: 3.7 } },
  { id: "b16", title: "Counting Storms",           author: "Bikram Shah",        year: 2022, status: "unread",                      format: "epub", pages: 384, addedDays: 18,  genres: ["Adventure", "Nature"],                            language: "en", isbn: "9780063143715", ratings: { google: 4.1, hardcover: 4.0, goodreads: 3.9 } },
  { id: "b17", title: "Sister Clay",               author: "Adaeze Nwankwo",     year: 2024, status: "unread",                      format: "epub", pages: 412, addedDays: 6,   genres: ["Literary", "Family"],                             language: "en", isbn: "9780525659808", ratings: { google: 4.6, hardcover: 4.5, goodreads: 4.4 } },
  { id: "b18", title: "The Weight of Pages",       author: "Henrik Sand",        year: 2016, status: "finished",                    format: "epub", pages: 528, addedDays: 612, genres: ["Mystery", "Crime"],            lastReadDays: 580, language: "no", isbn: "9788702165234", ratings: { google: 4.3, hardcover: 4.2, goodreads: 4.0 } },
  { id: "b19", title: "Permanent Daylight",        author: "Veda Kapoor",        year: 2023, status: "unread",                      format: "epub", pages: 256, addedDays: 8,   genres: ["Sci-fi"],                                         language: "en", isbn: "9780063295643", ratings: { google: 4.0, hardcover: 4.1, goodreads: 3.8 } },
  { id: "b20", title: "Field Manual for Ghosts",   author: "Cael Bishop",        year: 2021, status: "unread",                      format: "pdf",  pages: 176, addedDays: 32,  genres: ["Speculative", "Essay"],                           language: "en", isbn: "9781250276872", ratings: { google: 4.4, hardcover: 4.3, goodreads: 4.2 } },
  { id: "b21", title: "Provisional Coast",         author: "Ines Roca",          year: 2018, status: "unread",                      format: "epub", pages: 304, addedDays: 95,  genres: ["Literary"],                                       language: "es", isbn: "9788433925039", ratings: { google: 4.2, hardcover: 4.1, goodreads: 4.0 } },
  { id: "b22", title: "Lantern Pulse: Below",      author: "Yusra Al-Mansouri",  year: 2023, status: "unread",                      format: "epub", pages: 422, addedDays: 12,  series: { name: "Lantern Cycle", index: 2, total: 4 }, genres: ["Sci-fi", "Adventure"],         language: "en", isbn: "9780765395351", ratings: { google: 4.4, hardcover: 4.4, goodreads: 4.3 } },
  { id: "b23", title: "Lantern Pulse: Tower",      author: "Yusra Al-Mansouri",  year: 2024, status: "unread",                      format: "epub", pages: 446, addedDays: 7,   series: { name: "Lantern Cycle", index: 3, total: 4 }, genres: ["Sci-fi", "Adventure"],         language: "en", isbn: "9780765395368", ratings: { google: 4.5, hardcover: 4.5, goodreads: 4.4 } },
  { id: "b24", title: "Slow Engines",              author: "Mira Tanaka",        year: 2020, status: "finished",                    format: "epub", pages: 282, addedDays: 178, genres: ["Memoir", "Music"],             lastReadDays: 150, language: "en", isbn: "9781984899569", ratings: { google: 4.3, hardcover: 4.2, goodreads: 4.0 } },
  { id: "b25", title: "The Listening Room",        author: "Cael Bishop",        year: 2023, status: "unread",                      format: "epub", pages: 288, addedDays: 22,  genres: ["Speculative"],                                    language: "en", isbn: "9781250276889", ratings: { google: 4.1, hardcover: 4.0, goodreads: 3.9 } },
  { id: "b26", title: "Northern Static",           author: "Saskia Brandt",      year: 2019, status: "finished",                    format: "epub", pages: 376, addedDays: 320, genres: ["Architecture", "Essay"],       lastReadDays: 290, language: "de", isbn: "9783863354619", ratings: { google: 4.0, hardcover: 4.0, goodreads: 3.8 } },
  { id: "b27", title: "Honeycomb Cathedral",       author: "Adaeze Nwankwo",     year: 2021, status: "in-progress", progress: 0.61, format: "epub", pages: 504, addedDays: 41,  genres: ["Literary", "Family"],          lastReadDays: 8,   language: "en", isbn: "9780525659792", ratings: { google: 4.5, hardcover: 4.4, goodreads: 4.3 } },
  { id: "b28", title: "Without Anchor",            author: "Tomás Fialho",       year: 2024, status: "unread",                      format: "epub", pages: 248, addedDays: 4,   genres: ["Literary"],                                       language: "pt", isbn: "9789895624089", ratings: { google: 4.2, hardcover: 4.1, goodreads: 4.0 } },
];

export const SHELVES = {
  inProgress: BOOKS.filter((b) => b.status === "in-progress"),
  recentlyAdded: [...BOOKS].sort((a, b) => a.addedDays - b.addedDays).slice(0, 8),
  forgotten: BOOKS.filter((b) => b.status === "unread" && b.addedDays > 60).slice(0, 8),
  byYusra: BOOKS.filter((b) => b.author === "Yusra Al-Mansouri"),
  finishedThisYear: BOOKS.filter((b) => b.status === "finished").slice(0, 6),
};

export const USER_SHELVES = [
  { name: "Best of 2025", kind: "manual" as const, count: 12 },
  { name: "Slow reads", kind: "manual" as const, count: 7 },
  { name: "By Yusra Al-Mansouri", kind: "smart" as const, count: 3 },
  { name: "Lantern Cycle (incomplete)", kind: "smart" as const, count: 3 },
  { name: "Send to Kobo", kind: "device" as const, count: 5, device: "Kobo Libra 2", syncStatus: "pending" as const },
];

export const STATS = {
  totalBooks: 1247,
  read: 312,
  inProgress: 3,
  hoursThisYear: 184,
  pagesThisYear: 11420,
  finishedThisYear: 18,
};

export function bookHash(id: string): number {
  let h = 2166136261 >>> 0;
  for (let i = 0; i < id.length; i++) {
    h ^= id.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  return h;
}

export function bookHue(id: string): number {
  return bookHash(id) % 360;
}

export function bookTier(id: string): 0 | 1 | 2 | 3 | 4 {
  return ((bookHash(id) >>> 8) % 5) as 0 | 1 | 2 | 3 | 4;
}

export function initials(author: string): string {
  return author
    .split(/\s+/)
    .map((p) => p.charAt(0))
    .filter(Boolean)
    .slice(0, 2)
    .join("")
    .toUpperCase();
}

export function relDays(d?: number): string {
  if (d === undefined) return "—";
  if (d === 0) return "today";
  if (d === 1) return "yesterday";
  if (d < 7) return `${d}d`;
  if (d < 30) return `${Math.floor(d / 7)}w`;
  if (d < 365) return `${Math.floor(d / 30)}mo`;
  return `${Math.floor(d / 365)}y`;
}
