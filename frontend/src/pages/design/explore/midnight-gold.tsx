import { useState, useRef, useEffect, type ReactElement, type CSSProperties } from "react";
import { Link } from "react-router";
import "../../../design/explore/midnight-gold/tokens.css";
import {
  BOOKS,
  SHELVES,
  STATS,
  USER_SHELVES,
  bookHue,
  bookTier,
  relDays,
  type Book,
} from "./_shared/books";

type Theme = "dark" | "light";
type Mock = "home" | "detail" | "library";
type GridSize = "s" | "m" | "l";
type ViewMode = "grid" | "table";

function coverStyle(book: Book, theme: Theme): CSSProperties {
  const hue = bookHue(book.id);
  const tier = bookTier(book.id);
  // Warm-bias the hue: pull every cover towards 30°–60° (amber/ochre) range
  // so the palette family stays "Midnight Gold" while still varying per book.
  const warmHue = 22 + ((hue + tier * 11) % 28);
  const angle = (hue % 90) + 135;
  if (theme === "dark") {
    const top = `hsl(${warmHue}, 32%, ${26 + tier * 2}%)`;
    const bot = `hsl(${warmHue - 6}, 28%, ${12 + tier}%)`;
    return {
      background: `linear-gradient(${angle}deg, ${top} 0%, ${bot} 90%)`,
      color: "#ECE3D0",
    };
  }
  const top = `hsl(${warmHue}, 38%, ${74 - tier * 2}%)`;
  const bot = `hsl(${warmHue - 6}, 36%, ${52 - tier * 2}%)`;
  return {
    background: `linear-gradient(${angle}deg, ${top} 0%, ${bot} 90%)`,
    color: "#2A231A",
  };
}

function coverBackdropStyle(book: Book, theme: Theme): CSSProperties {
  const hue = bookHue(book.id);
  // Warm-clamp hue range matches the cover palette family: amber/ochre.
  const warmHue = 22 + (hue % 28);
  if (theme === "dark") {
    return {
      background: `radial-gradient(60% 50% at 30% 30%, hsl(${warmHue}, 32%, 22%), transparent 68%),
                   radial-gradient(55% 45% at 75% 60%, hsl(${warmHue + 10}, 26%, 18%), transparent 70%),
                   #14130E`,
    };
  }
  // Light: pale warm tints on cream canvas, much lower contrast so the
  // backdrop reads as warmth, not a muddy wash that drowns the foreground.
  return {
    background: `radial-gradient(60% 50% at 30% 30%, hsl(${warmHue}, 38%, 86%), transparent 72%),
                 radial-gradient(55% 45% at 75% 60%, hsl(${warmHue + 10}, 32%, 80%), transparent 72%),
                 #F4ECDC`,
  };
}

interface CoverProps {
  book: Book;
  theme: Theme;
  showAuthor?: boolean;
}

function Cover({ book, theme, showAuthor = true }: CoverProps): ReactElement {
  return (
    <div className="mg-tile-cover">
      <div className="mg-tile-cover-inner" style={coverStyle(book, theme)}>
        <div>
          <div className="mg-tile-cover-rule" />
          {showAuthor && <div className="mg-tile-cover-author">{book.author}</div>}
        </div>
        <div className="mg-tile-cover-title">{book.title}</div>
      </div>
    </div>
  );
}

interface TileProps {
  book: Book;
  theme: Theme;
  size?: "default" | "lg" | "sm";
  showStatus?: boolean;
}

function Tile({ book, theme, size = "default", showStatus = false }: TileProps): ReactElement {
  const cls = size === "lg" ? "mg-tile mg-tile-lg" : size === "sm" ? "mg-tile mg-tile-sm" : "mg-tile";
  return (
    <article className={cls}>
      <Cover book={book} theme={theme} />
      <div className="mg-tile-meta">
        <h4 className="mg-tile-title">{book.title}</h4>
        <div className="mg-tile-author">{book.author}</div>
        {book.status === "in-progress" && book.progress !== undefined && (
          <div className="mg-tile-progress" aria-label={`${Math.round(book.progress * 100)}% read`}>
            <div className="mg-tile-progress-fill" style={{ width: `${book.progress * 100}%` }} />
          </div>
        )}
        {showStatus && book.status !== "unread" && (
          <div className="mg-tile-status">
            <span className={`mg-status-dot ${book.status}`} />
            {book.status === "in-progress"
              ? `${Math.round((book.progress ?? 0) * 100)}% read`
              : "Finished"}
          </div>
        )}
      </div>
    </article>
  );
}

interface MarqueeProps {
  children: ReactElement[];
  speed?: number;
  reverse?: boolean;
}

function Marquee({ children, speed = 80, reverse = false }: MarqueeProps): ReactElement {
  const duplicated = children.map((child, i) =>
    typeof child === "object" && child !== null && "props" in child
      ? { ...child, key: `b-${i}` }
      : child,
  );
  const styleVar: CSSProperties = {
    animationDuration: `${speed}s`,
    animationDirection: reverse ? "reverse" : "normal",
  };
  return (
    <div className="mg-marquee" aria-roledescription="carousel">
      <div className="mg-marquee-track" style={styleVar}>
        <div className="mg-marquee-set">{children}</div>
        <div className="mg-marquee-set" aria-hidden="true">{duplicated}</div>
      </div>
    </div>
  );
}

interface HomeProps {
  theme: Theme;
}

function Home({ theme }: HomeProps): ReactElement {
  const featured = SHELVES.inProgress[0] ?? BOOKS[0];
  const heroRef = useRef<HTMLElement>(null);
  const backdropRef = useRef<HTMLDivElement>(null);
  const stackRef = useRef<HTMLDivElement>(null);
  const heroCoverStack = SHELVES.inProgress.slice(0, 3);

  useEffect(() => {
    const hero = heroRef.current;
    const backdrop = backdropRef.current;
    const stack = stackRef.current;
    if (!hero) return;
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;

    let frame = 0;
    const onMove = (e: MouseEvent) => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const rect = hero.getBoundingClientRect();
        const x = (e.clientX - rect.left) / rect.width - 0.5;
        const y = (e.clientY - rect.top) / rect.height - 0.5;
        if (backdrop) {
          backdrop.style.transform = `translate(${x * -32}px, ${y * -22}px) scale(1.08)`;
        }
        if (stack) {
          stack.style.transform = `translate(${x * 18}px, ${y * 14}px)`;
          const tiles = stack.querySelectorAll<HTMLDivElement>("[data-depth]");
          tiles.forEach((tile) => {
            const d = parseFloat(tile.dataset.depth ?? "0");
            tile.style.transform = `translate(${x * d * -8}px, ${y * d * -6}px)`;
          });
        }
      });
    };
    const onLeave = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (backdrop) backdrop.style.transform = "translate(0, 0) scale(1.08)";
        if (stack) {
          stack.style.transform = "translate(0, 0)";
          stack.querySelectorAll<HTMLDivElement>("[data-depth]").forEach((tile) => {
            tile.style.transform = "translate(0, 0)";
          });
        }
      });
    };
    hero.addEventListener("mousemove", onMove);
    hero.addEventListener("mouseleave", onLeave);
    return () => {
      hero.removeEventListener("mousemove", onMove);
      hero.removeEventListener("mouseleave", onLeave);
      cancelAnimationFrame(frame);
    };
  }, []);

  return (
    <>
      <section className="mg-hero" ref={heroRef}>
        <div className="mg-hero-backdrop" ref={backdropRef} style={coverBackdropStyle(featured, theme)} />
        <div className="mg-hero-grid">
          <div className="mg-hero-content">
            <div className="mg-eyebrow">Wednesday evening · Continue</div>
            <h1 className="mg-hero-h">
              Two pages back into <em>{featured.title}</em>.
            </h1>
            <p className="mg-hero-sub">
              You left off at chapter twelve. {Math.round((featured.progress ?? 0) * 100)} percent of
              the way through. Pick it up where you stopped.
            </p>
            <button className="mg-hero-cta" type="button">
              Resume reading
              <span aria-hidden="true">→</span>
            </button>
            <div className="mg-hero-meta">
              <span><strong>{STATS.totalBooks.toLocaleString()}</strong> books in library</span>
              <span><strong>{STATS.inProgress}</strong> in progress</span>
              <span><strong>{STATS.finishedThisYear}</strong> finished this year</span>
            </div>
          </div>
          <div className="mg-hero-stack" ref={stackRef} aria-hidden="true">
            {heroCoverStack.map((b, i) => (
              <div
                key={b.id}
                className="mg-hero-stack-tile"
                data-depth={i === 0 ? 3 : i === 1 ? 1.6 : 0.8}
                data-position={i}
              >
                <div className="mg-tile-cover">
                  <div className="mg-tile-cover-inner" style={coverStyle(b, theme)}>
                    <div>
                      <div className="mg-tile-cover-rule" />
                      <div className="mg-tile-cover-author">{b.author}</div>
                    </div>
                    <div className="mg-tile-cover-title">{b.title}</div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section className="mg-shelf">
        <div className="mg-shelf-head">
          <h2 className="mg-shelf-title">In progress</h2>
          <div className="mg-shelf-meta">
            <span>{SHELVES.inProgress.length} active</span>
          </div>
        </div>
        <Marquee speed={70} reverse={false}>
          {SHELVES.inProgress.map((b, i) => (
            <Tile key={`a-${b.id}-${i}`} book={b} theme={theme} size="lg" showStatus />
          ))}
        </Marquee>
      </section>

      <section className="mg-shelf">
        <div className="mg-shelf-head">
          <h2 className="mg-shelf-title">Recently added</h2>
          <div className="mg-shelf-meta">
            <a href="#">See all →</a>
          </div>
        </div>
        <Marquee speed={90} reverse={true}>
          {SHELVES.recentlyAdded.map((b, i) => (
            <Tile key={`a-${b.id}-${i}`} book={b} theme={theme} />
          ))}
        </Marquee>
      </section>

      <section className="mg-shelf">
        <div className="mg-shelf-head">
          <h2 className="mg-shelf-title">
            Forgotten favourites <em>·</em> from your library
          </h2>
          <div className="mg-shelf-meta">
            <span className="mg-badge">Discovery</span>
          </div>
        </div>
        <Marquee speed={80} reverse={false}>
          {SHELVES.forgotten.map((b, i) => (
            <Tile key={`a-${b.id}-${i}`} book={b} theme={theme} />
          ))}
        </Marquee>
      </section>

      <section className="mg-shelf">
        <div className="mg-shelf-head">
          <h2 className="mg-shelf-title">Lantern Cycle <em>·</em> incomplete series</h2>
          <div className="mg-shelf-meta">
            <span className="mg-badge">Smart shelf</span>
          </div>
        </div>
        <Marquee speed={75} reverse={true}>
          {SHELVES.byYusra.map((b, i) => (
            <Tile key={`a-${b.id}-${i}`} book={b} theme={theme} showStatus />
          ))}
        </Marquee>
      </section>

      <section className="mg-stats">
        <div>
          <div className="mg-stat-num">
            <em>{STATS.totalBooks.toLocaleString()}</em>
          </div>
          <div className="mg-stat-label">Books in library</div>
        </div>
        <div>
          <div className="mg-stat-num">{STATS.read}</div>
          <div className="mg-stat-label">Read all-time</div>
        </div>
        <div>
          <div className="mg-stat-num">{STATS.hoursThisYear}h</div>
          <div className="mg-stat-label">Read this year</div>
        </div>
        <div>
          <div className="mg-stat-num">{STATS.pagesThisYear.toLocaleString()}</div>
          <div className="mg-stat-label">Pages this year</div>
        </div>
      </section>
    </>
  );
}

function Detail({ theme }: { theme: Theme }): ReactElement {
  const book = BOOKS.find((b) => b.id === "b02") ?? BOOKS[0];
  const moreByAuthor = BOOKS.filter((b) => b.author === book.author && b.id !== book.id).slice(0, 4);
  return (
    <article className="mg-detail">
      <div className="mg-detail-backdrop" style={coverBackdropStyle(book, theme)} />
      <div className="mg-detail-grid">
        <aside>
          <div className="mg-detail-cover">
            <div className="mg-tile-cover-inner" style={coverStyle(book, theme)}>
              <div>
                <div className="mg-tile-cover-rule" />
                <div className="mg-tile-cover-author">{book.author}</div>
              </div>
              <div className="mg-tile-cover-title">{book.title}</div>
            </div>
          </div>
          <dl className="mg-detail-aside">
            <div>
              <dt>Format</dt>
              <dd>{book.format.toUpperCase()} · {book.pages} pages</dd>
            </div>
            <div>
              <dt>Published</dt>
              <dd>{book.year}</dd>
            </div>
            <div>
              <dt>Added</dt>
              <dd>{book.addedDays} days ago</dd>
            </div>
            <div>
              <dt>Status</dt>
              <dd>
                {book.status === "in-progress"
                  ? `Reading · ${Math.round((book.progress ?? 0) * 100)}% complete`
                  : book.status === "finished"
                    ? "Finished"
                    : "Unread"}
              </dd>
            </div>
          </dl>
        </aside>
        <div>
          <div className="mg-detail-eyebrow">Reverie · Detail view</div>
          <h1 className="mg-detail-title">
            <em>Salt</em> and Cipher
          </h1>
          <p className="mg-detail-byline">by {book.author}</p>
          <div className="mg-detail-actions">
            <button className="mg-hero-cta" type="button">
              Resume at 78%
              <span aria-hidden="true">→</span>
            </button>
            <button className="mg-button-secondary" type="button">Add to shelf</button>
            <button className="mg-button-secondary" type="button">Send to Kobo</button>
          </div>
          <div className="mg-detail-summary">
            <p>
              An archivist on a coastal research station begins decoding a sequence of letters that
              appear, then disappear, in the salt residue at the bottom of the brackish-water jars
              kept along the south wall. The sender is unknown. The cipher resists every method she
              owns. Over six months she trains herself to wait — to let the letters resolve in the
              order they want — and the book turns, gradually, into a study of patient attention.
            </p>
            <p>
              A second narrative thread tracks her grandfather, who built the station in the 1960s
              and may have been the cipher's original author. The book moves between the two voices
              in measured intervals, and refuses to confirm.
            </p>
          </div>
          <section className="mg-detail-section">
            <h3>More by {book.author}</h3>
            <div className="mg-detail-row">
              {moreByAuthor.map((b) => (
                <Tile key={b.id} book={b} theme={theme} size="sm" />
              ))}
              {moreByAuthor.length === 0 && (
                <p style={{ color: "var(--mg-fg-faint)", fontSize: "var(--mg-type-small)" }}>
                  This is the only {book.author} title in your library.
                </p>
              )}
            </div>
          </section>
        </div>
      </div>
    </article>
  );
}

type ColumnId =
  | "title" | "author" | "series" | "seriesNum" | "genres"
  | "lastRead" | "added" | "progress"
  | "language" | "pages" | "isbn"
  | "ratingGoogle" | "ratingHardcover" | "ratingGoodreads";

interface ColumnDef {
  id: ColumnId;
  label: string;
  optional?: boolean;
}

const COLUMNS: ColumnDef[] = [
  { id: "title", label: "Title" },
  { id: "author", label: "Author" },
  { id: "series", label: "Series" },
  { id: "seriesNum", label: "#" },
  { id: "genres", label: "Genre(s)" },
  { id: "lastRead", label: "Last read" },
  { id: "added", label: "Added" },
  { id: "progress", label: "Progress" },
  { id: "language", label: "Lang", optional: true },
  { id: "pages", label: "Pages", optional: true },
  { id: "isbn", label: "ISBN", optional: true },
  { id: "ratingGoogle", label: "G", optional: true },
  { id: "ratingHardcover", label: "Hc", optional: true },
  { id: "ratingGoodreads", label: "GR", optional: true },
];

const DEFAULT_VISIBLE: ColumnId[] = [
  "title", "author", "series", "seriesNum", "genres", "lastRead", "added", "progress",
];

const DEFAULT_SORT: { col: ColumnId; dir: "asc" | "desc" }[] = [
  { col: "lastRead", dir: "desc" },
  { col: "author", dir: "asc" },
];

function ratingCell(v?: number): ReactElement {
  if (v === undefined) return <span className="mg-fg-faint">—</span>;
  return <span style={{ fontVariantNumeric: "tabular-nums" }}>{v.toFixed(1)}</span>;
}

function Library({ theme }: { theme: Theme }): ReactElement {
  const [size, setSize] = useState<GridSize>("m");
  const [view, setView] = useState<ViewMode>("table");
  const [shelf, setShelf] = useState<string>("All");
  const [visibleCols, setVisibleCols] = useState<Set<ColumnId>>(new Set(DEFAULT_VISIBLE));
  const [columnsOpen, setColumnsOpen] = useState(false);
  const [sortStack] = useState(DEFAULT_SORT);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const shelves = ["All", ...USER_SHELVES.map((s) => s.name)];
  const cols = COLUMNS.filter((c) => visibleCols.has(c.id));

  const sorted = [...BOOKS].sort((a, b) => {
    for (const { col, dir } of sortStack) {
      const av = sortValue(a, col);
      const bv = sortValue(b, col);
      const cmp = av < bv ? -1 : av > bv ? 1 : 0;
      if (cmp !== 0) return dir === "asc" ? cmp : -cmp;
    }
    return 0;
  });

  const allSelected = selected.size === sorted.length && sorted.length > 0;
  const someSelected = selected.size > 0 && !allSelected;

  const toggleRow = (id: string) => {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelected(next);
  };

  const toggleAll = () => {
    if (selected.size === 0) setSelected(new Set(sorted.map((b) => b.id)));
    else setSelected(new Set());
  };

  return (
    <div className="mg-library">
      <div className="mg-library-head">
        <div>
          <div className="mg-eyebrow">{BOOKS.length} of {STATS.totalBooks.toLocaleString()}</div>
          <h1 className="mg-library-title">
            Your <em>library</em>
          </h1>
        </div>
        <div className="mg-library-controls">
          <div className="mg-control-group" role="group" aria-label="Tile size">
            <button type="button" aria-pressed={size === "s"} onClick={() => setSize("s")}>S</button>
            <button type="button" aria-pressed={size === "m"} onClick={() => setSize("m")}>M</button>
            <button type="button" aria-pressed={size === "l"} onClick={() => setSize("l")}>L</button>
          </div>
          <div className="mg-control-group" role="group" aria-label="View">
            <button type="button" aria-pressed={view === "grid"} onClick={() => setView("grid")}>Tiles</button>
            <button type="button" aria-pressed={view === "table"} onClick={() => setView("table")}>Table</button>
          </div>
          {view === "table" && (
            <div className="mg-columns-menu">
              <button
                type="button"
                className="mg-button-secondary"
                onClick={() => setColumnsOpen((v) => !v)}
                aria-expanded={columnsOpen}
              >
                Columns · {visibleCols.size}/{COLUMNS.length}
              </button>
              {columnsOpen && (
                <div className="mg-columns-pop">
                  <div className="mg-columns-pop-head">Visible columns</div>
                  {COLUMNS.map((c) => (
                    <label key={c.id} className="mg-columns-row">
                      <input
                        type="checkbox"
                        checked={visibleCols.has(c.id)}
                        onChange={() => {
                          const next = new Set(visibleCols);
                          if (next.has(c.id)) next.delete(c.id);
                          else next.add(c.id);
                          setVisibleCols(next);
                        }}
                      />
                      <span>{c.label}</span>
                      {c.optional && <span className="mg-columns-row-tag">optional</span>}
                    </label>
                  ))}
                  <div className="mg-columns-pop-foot">Saved to your account</div>
                </div>
              )}
            </div>
          )}
          <button className="mg-button-secondary" type="button">+ New shelf</button>
        </div>
      </div>

      <div className="mg-library-shelves" role="tablist">
        {shelves.map((s) => {
          const meta = USER_SHELVES.find((u) => u.name === s);
          const icon = meta?.kind === "smart" ? "Auto" : meta?.kind === "device" ? "Sync" : null;
          return (
            <button
              key={s}
              type="button"
              role="tab"
              className="mg-shelf-chip"
              aria-pressed={shelf === s}
              onClick={() => setShelf(s)}
            >
              {icon && <span className="mg-shelf-chip-icon">{icon}</span>}
              {s}
              {meta && <span style={{ opacity: 0.5, marginLeft: 6 }}>{meta.count}</span>}
            </button>
          );
        })}
      </div>

      {view === "table" && selected.size === 0 && (
        <div className="mg-sort-bar">
          <span className="mg-sort-bar-label">Sorted by</span>
          {sortStack.map((s, i) => {
            const col = COLUMNS.find((c) => c.id === s.col);
            return (
              <span key={s.col} className="mg-sort-chip">
                {i > 0 && <span className="mg-sort-then">then</span>}
                {col?.label} <span className="mg-sort-arrow">{s.dir === "asc" ? "↑" : "↓"}</span>
              </span>
            );
          })}
          <button type="button" className="mg-sort-edit">Edit sort →</button>
        </div>
      )}

      {view === "table" && selected.size > 0 && (
        <div className="mg-bulk-bar">
          <span className="mg-bulk-count">
            <strong>{selected.size}</strong> selected
          </span>
          <span className="mg-bulk-sep" aria-hidden="true">·</span>
          <button type="button" className="mg-bulk-action">Add to shelf</button>
          <button type="button" className="mg-bulk-action">Send to device</button>
          <button type="button" className="mg-bulk-action">Mark as read</button>
          <button type="button" className="mg-bulk-action">Edit metadata</button>
          <button type="button" className="mg-bulk-action danger">Remove from library</button>
          <button
            type="button"
            className="mg-bulk-clear"
            onClick={() => setSelected(new Set())}
          >
            Clear
          </button>
        </div>
      )}

      {view === "grid" ? (
        <div className="mg-grid" data-size={size}>
          {sorted.map((b) => (
            <Tile key={b.id} book={b} theme={theme} showStatus />
          ))}
        </div>
      ) : (
        <div className="mg-table-wrap">
          <table className="mg-table">
            <thead>
              <tr>
                <th className="col-select">
                  <input
                    type="checkbox"
                    aria-label={allSelected ? "Deselect all" : "Select all"}
                    checked={allSelected}
                    ref={(el) => { if (el) el.indeterminate = someSelected; }}
                    onChange={toggleAll}
                  />
                </th>
                {cols.map((c) => {
                  const sort = sortStack.find((s) => s.col === c.id);
                  const cls = ["sortable", sort ? `sorted-${sort.dir}` : ""].join(" ");
                  return <th key={c.id} className={cls}>{c.label}</th>;
                })}
              </tr>
            </thead>
            <tbody>
              {sorted.map((b) => {
                const isSelected = selected.has(b.id);
                return (
                  <tr key={b.id} className={isSelected ? "selected" : undefined}>
                    <td className="col-select">
                      <input
                        type="checkbox"
                        aria-label={`Select ${b.title}`}
                        checked={isSelected}
                        onChange={() => toggleRow(b.id)}
                      />
                    </td>
                    {cols.map((c) => (
                      <td key={c.id} className={cellClass(c.id)}>
                        {renderCell(b, c.id, theme)}
                      </td>
                    ))}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function cellClass(id: ColumnId): string {
  switch (id) {
    case "title": return "col-title";
    case "pages":
    case "ratingGoogle":
    case "ratingHardcover":
    case "ratingGoodreads":
    case "seriesNum":
      return "col-num";
    default: return "";
  }
}

function sortValue(b: Book, id: ColumnId): number | string {
  switch (id) {
    case "title": return b.title.toLowerCase();
    case "author": return b.author.toLowerCase();
    case "series": return b.series?.name.toLowerCase() ?? "￿";
    case "seriesNum": return b.series?.index ?? 999;
    case "genres": return b.genres?.[0]?.toLowerCase() ?? "￿";
    case "lastRead": return b.lastReadDays ?? 99999;
    case "added": return b.addedDays;
    case "progress": return b.progress ?? (b.status === "finished" ? 1 : 0);
    case "language": return b.language ?? "";
    case "pages": return b.pages;
    case "isbn": return b.isbn ?? "";
    case "ratingGoogle": return b.ratings?.google ?? 0;
    case "ratingHardcover": return b.ratings?.hardcover ?? 0;
    case "ratingGoodreads": return b.ratings?.goodreads ?? 0;
  }
}

function renderCell(b: Book, id: ColumnId, theme: Theme): ReactElement {
  switch (id) {
    case "title":
      return (
        <span style={{ display: "inline-flex", alignItems: "center" }}>
          <span className="mg-mini-cover" style={coverStyle(b, theme)} aria-hidden="true" />
          <span>
            <span className="title">{b.title}</span>
            {b.format === "pdf" && <span className="mg-tag-format" style={{ marginLeft: 6 }}>PDF</span>}
          </span>
        </span>
      );
    case "author":
      return <span>{b.author}</span>;
    case "series":
      return b.series ? <span>{b.series.name}</span> : <span style={{ color: "var(--mg-fg-faint)" }}>—</span>;
    case "seriesNum":
      return b.series ? <span>{b.series.index}/{b.series.total}</span> : <span style={{ color: "var(--mg-fg-faint)" }}>—</span>;
    case "genres":
      return <span>{b.genres?.join(", ") ?? "—"}</span>;
    case "lastRead":
      return <span>{relDays(b.lastReadDays)}</span>;
    case "added":
      return <span>{relDays(b.addedDays)}</span>;
    case "progress":
      if (b.status === "finished") {
        return <span style={{ color: "var(--mg-success)" }}>✓ Read</span>;
      }
      if (b.status === "unread") {
        return <span style={{ color: "var(--mg-fg-faint)" }}>—</span>;
      }
      return (
        <span style={{ display: "inline-flex", alignItems: "center" }}>
          <span className="mg-progress-bar"><span className="mg-progress-bar-fill" style={{ width: `${(b.progress ?? 0) * 100}%` }} /></span>
          <span style={{ fontVariantNumeric: "tabular-nums" }}>{Math.round((b.progress ?? 0) * 100)}%</span>
        </span>
      );
    case "language":
      return <span style={{ textTransform: "uppercase", fontFamily: "var(--mg-font-mono)", fontSize: 11 }}>{b.language ?? "—"}</span>;
    case "pages":
      return <span style={{ fontVariantNumeric: "tabular-nums" }}>{b.pages}</span>;
    case "isbn":
      return <span style={{ fontFamily: "var(--mg-font-mono)", fontSize: 11, color: "var(--mg-fg-muted)" }}>{b.isbn ?? "—"}</span>;
    case "ratingGoogle":
      return ratingCell(b.ratings?.google);
    case "ratingHardcover":
      return ratingCell(b.ratings?.hardcover);
    case "ratingGoodreads":
      return ratingCell(b.ratings?.goodreads);
  }
}

export default function MidnightGold(): ReactElement {
  const [theme, setTheme] = useState<Theme>("dark");
  const [mock, setMock] = useState<Mock>("home");

  return (
    <div className="mg-root" data-theme={theme}>
      <header className="mg-topbar">
        <div className="mg-wordmark">
          Reverie<span>.</span>
        </div>
        <nav className="mg-nav" aria-label="Primary">
          <a href="#" aria-current="page">Library</a>
          <a href="#">Shelves</a>
          <a href="#">Reader</a>
          <a href="#">Stats</a>
        </nav>
        <div className="mg-spacer" />
        <div className="mg-search" aria-label="Search">
          <span aria-hidden="true">⌕</span>
          <span>Search title, author, shelf</span>
          <kbd>⌘K</kbd>
        </div>
        <button className="mg-iconbtn" type="button" aria-label="Settings">⚙</button>
        <div className="mg-themetoggle" role="group" aria-label="Theme">
          <button type="button" aria-pressed={theme === "dark"} onClick={() => setTheme("dark")}>Dark</button>
          <button type="button" aria-pressed={theme === "light"} onClick={() => setTheme("light")}>Light</button>
        </div>
        <div className="mg-avatar" aria-label="User menu">JU</div>
      </header>

      <div className="mg-mocktabs" role="tablist" aria-label="Mock screen">
        <button type="button" role="tab" className="mg-mocktab" aria-pressed={mock === "home"} onClick={() => setMock("home")}>
          <span>01</span> Home dashboard
        </button>
        <button type="button" role="tab" className="mg-mocktab" aria-pressed={mock === "detail"} onClick={() => setMock("detail")}>
          <span>02</span> Book detail
        </button>
        <button type="button" role="tab" className="mg-mocktab" aria-pressed={mock === "library"} onClick={() => setMock("library")}>
          <span>03</span> Library full-grid
        </button>
        <div className="mg-spacer" />
        <Link
          to="/design/explore"
          style={{
            color: "var(--mg-fg-faint)",
            fontSize: "var(--mg-type-small)",
            letterSpacing: "var(--mg-tracking-chrome)",
            textTransform: "uppercase",
            textDecoration: "none",
            alignSelf: "center",
          }}
        >
          ← All directions
        </Link>
      </div>

      {mock === "home" && <Home theme={theme} />}
      {mock === "detail" && <Detail theme={theme} />}
      {mock === "library" && <Library theme={theme} />}
    </div>
  );
}
