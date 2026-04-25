import { useState, type ReactElement, type CSSProperties } from "react";
import { Link } from "react-router";
import "../../../design/explore/atelier-ink/tokens.css";
import {
  BOOKS,
  SHELVES,
  STATS,
  USER_SHELVES,
  bookHue,
  bookTier,
  type Book,
} from "./_shared/books";

type Theme = "dark" | "light";
type Mock = "home" | "detail" | "library";
type GridSize = "s" | "m" | "l";
type ViewMode = "grid" | "table";
type Accent = "chartreuse" | "persimmon" | "carmine";

function coverStyle(book: Book, theme: Theme): CSSProperties {
  const hue = bookHue(book.id);
  const tier = bookTier(book.id);
  if (theme === "dark") {
    const top = `hsl(${hue}, 18%, ${24 + tier * 2}%)`;
    const bot = `hsl(${hue}, 14%, ${12 + tier}%)`;
    return {
      background: `linear-gradient(160deg, ${top} 0%, ${bot} 100%)`,
      color: "#EFEAE0",
    };
  }
  const top = `hsl(${hue}, 22%, ${78 - tier * 2}%)`;
  const bot = `hsl(${hue}, 18%, ${58 - tier * 2}%)`;
  return {
    background: `linear-gradient(160deg, ${top} 0%, ${bot} 100%)`,
    color: "#1A1812",
  };
}

interface CoverProps {
  book: Book;
  theme: Theme;
}

function Cover({ book, theme }: CoverProps): ReactElement {
  const num = book.id.replace("b", "");
  return (
    <div className="ai-tile-cover">
      <div className="ai-tile-cover-inner" style={coverStyle(book, theme)}>
        <div className="ai-tile-cover-corner">no. {num}</div>
        <div>
          <div className="ai-tile-cover-title">{book.title}</div>
          <div className="ai-tile-cover-rule" />
          <div className="ai-tile-cover-author">{book.author}</div>
        </div>
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
  const cls = size === "lg" ? "ai-tile ai-tile-lg" : size === "sm" ? "ai-tile ai-tile-sm" : "ai-tile";
  return (
    <article className={cls}>
      <Cover book={book} theme={theme} />
      <div className="ai-tile-meta">
        <h4 className="ai-tile-title">{book.title}</h4>
        <div className="ai-tile-author">{book.author}</div>
        {showStatus && (
          <div className="ai-tile-status-row">
            <span>
              <span className={`ai-status-dot ${book.status}`} />
              {book.status === "in-progress"
                ? `${Math.round((book.progress ?? 0) * 100)}%`
                : book.status === "finished" ? "Read" : "·"}
            </span>
            <span>{book.year}</span>
          </div>
        )}
      </div>
    </article>
  );
}

function Home({ theme }: { theme: Theme }): ReactElement {
  const featuredFinish = SHELVES.recentlyAdded[0];
  const featuredShelf = SHELVES.recentlyAdded.slice(1, 4);

  return (
    <>
      <section className="ai-hero">
        <div>
          <div className="ai-eyebrow">Wednesday · evening browse</div>
          <h1 className="ai-hero-h">
            What's <em>next</em>?
          </h1>
          <ol className="ai-hero-list">
            {SHELVES.inProgress.slice(0, 3).map((b, i) => (
              <li key={b.id}>
                <span className="ai-li-num">{String(i + 1).padStart(2, "0")}</span>
                <span>
                  <div className="ai-li-title">{b.title}</div>
                  <div className="ai-li-author">{b.author}</div>
                </span>
                <span className="ai-li-meta">
                  {b.progress !== undefined ? `${Math.round(b.progress * 100)}%` : "Begin"}
                </span>
              </li>
            ))}
          </ol>
          <button className="ai-hero-cta" type="button">
            Resume reading
            <span aria-hidden="true">→</span>
          </button>
        </div>
        <aside className="ai-hero-aside">
          <div className="ai-eyebrow" style={{ marginBottom: 0 }}>In progress</div>
          <div className="ai-hero-stack">
            {SHELVES.inProgress.slice(0, 3).map((b) => (
              <Cover key={b.id} book={b} theme={theme} />
            ))}
          </div>
        </aside>
      </section>

      <section className="ai-shelf">
        <div className="ai-shelf-head">
          <h2 className="ai-shelf-title">
            Recently <em>added</em>
          </h2>
          <div className="ai-shelf-meta">
            <a href="#">View all →</a>
          </div>
        </div>
        <div className="ai-featured-row">
          {featuredFinish && (
            <article className="ai-featured">
              <div className="ai-featured-cover">
                <div className="ai-tile-cover-inner" style={coverStyle(featuredFinish, theme)}>
                  <div className="ai-tile-cover-corner">no. {featuredFinish.id.replace("b", "")}</div>
                  <div>
                    <div className="ai-tile-cover-title" style={{ fontSize: 22 }}>{featuredFinish.title}</div>
                    <div className="ai-tile-cover-rule" />
                    <div className="ai-tile-cover-author">{featuredFinish.author}</div>
                  </div>
                </div>
              </div>
              <div>
                <div className="ai-featured-eyebrow">{featuredFinish.addedDays} days ago · feature</div>
                <h3 className="ai-featured-title">{featuredFinish.title}</h3>
                <p className="ai-featured-byline">by {featuredFinish.author} · {featuredFinish.year}</p>
              </div>
            </article>
          )}
          {featuredShelf.map((b) => (
            <Tile key={b.id} book={b} theme={theme} showStatus />
          ))}
        </div>
      </section>

      <section className="ai-shelf">
        <div className="ai-shelf-head">
          <h2 className="ai-shelf-title">
            Forgotten <em>favourites</em>
          </h2>
          <div className="ai-shelf-meta">
            <span className="ai-tag">Discovery</span>
          </div>
        </div>
        <div className="ai-carousel">
          {SHELVES.forgotten.map((b) => (
            <Tile key={b.id} book={b} theme={theme} />
          ))}
        </div>
      </section>

      <section className="ai-shelf">
        <div className="ai-shelf-head">
          <h2 className="ai-shelf-title">
            Lantern Cycle · <em>incomplete</em>
          </h2>
          <div className="ai-shelf-meta">
            <span className="ai-tag">Smart shelf</span>
          </div>
        </div>
        <div className="ai-carousel">
          {SHELVES.byYusra.map((b) => (
            <Tile key={b.id} book={b} theme={theme} showStatus />
          ))}
        </div>
      </section>

      <section className="ai-shelf">
        <div className="ai-shelf-head">
          <h2 className="ai-shelf-title">
            Library · <em>2026</em>
          </h2>
          <div className="ai-shelf-meta">
            {STATS.finishedThisYear} finished · {STATS.hoursThisYear}h read · {STATS.pagesThisYear.toLocaleString()} pages
          </div>
        </div>
        <div className="ai-carousel">
          {SHELVES.finishedThisYear.map((b) => (
            <Tile key={b.id} book={b} theme={theme} showStatus />
          ))}
        </div>
      </section>
    </>
  );
}

function Detail({ theme }: { theme: Theme }): ReactElement {
  const book = BOOKS.find((b) => b.id === "b02") ?? BOOKS[0];
  const moreByAuthor = BOOKS.filter((b) => b.author === book.author && b.id !== book.id).slice(0, 4);
  return (
    <article className="ai-detail">
      <div className="ai-detail-grid">
        <aside>
          <div className="ai-detail-cover">
            <div className="ai-tile-cover-inner" style={coverStyle(book, theme)}>
              <div className="ai-tile-cover-corner">no. {book.id.replace("b", "")}</div>
              <div>
                <div className="ai-tile-cover-title">{book.title}</div>
                <div className="ai-tile-cover-rule" />
                <div className="ai-tile-cover-author">{book.author}</div>
              </div>
            </div>
          </div>
          <dl className="ai-detail-aside">
            <div className="ai-aside-row"><dt>Format</dt><dd>{book.format.toUpperCase()}</dd></div>
            <div className="ai-aside-row"><dt>Pages</dt><dd>{book.pages}</dd></div>
            <div className="ai-aside-row"><dt>Published</dt><dd>{book.year}</dd></div>
            <div className="ai-aside-row"><dt>Added</dt><dd>{book.addedDays} days ago</dd></div>
            <div className="ai-aside-row">
              <dt>Status</dt>
              <dd className="accent">
                {book.status === "in-progress"
                  ? `${Math.round((book.progress ?? 0) * 100)}% read`
                  : book.status === "finished" ? "Finished" : "Unread"}
              </dd>
            </div>
          </dl>
        </aside>
        <div>
          <div className="ai-detail-eyebrow">Reverie · Detail · {book.id}</div>
          <h1 className="ai-detail-title">
            <em>Salt</em> and Cipher
          </h1>
          <p className="ai-detail-byline">by {book.author}</p>
          <div className="ai-detail-actions">
            <button className="ai-hero-cta" type="button">
              Resume at 78%
              <span aria-hidden="true">→</span>
            </button>
            <button className="ai-button-secondary" type="button">Add to shelf</button>
            <button className="ai-button-secondary" type="button">Send to Kobo</button>
            <button className="ai-button-secondary" type="button">Edit metadata</button>
          </div>
          <div className="ai-detail-summary">
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
          <section className="ai-detail-section">
            <h3>More <em>by</em> {book.author}</h3>
            <div className="ai-detail-row">
              {moreByAuthor.map((b) => (
                <Tile key={b.id} book={b} theme={theme} size="sm" />
              ))}
            </div>
          </section>
        </div>
      </div>
    </article>
  );
}

function Library({ theme }: { theme: Theme }): ReactElement {
  const [size, setSize] = useState<GridSize>("m");
  const [view, setView] = useState<ViewMode>("grid");
  const [shelf, setShelf] = useState<string>("All");

  const shelves = ["All", ...USER_SHELVES.map((s) => s.name)];

  return (
    <div className="ai-library">
      <div className="ai-library-head">
        <div>
          <div className="ai-eyebrow">{BOOKS.length} of {STATS.totalBooks.toLocaleString()}</div>
          <h1 className="ai-library-title">
            Your <em>library</em>
          </h1>
        </div>
        <div className="ai-library-controls">
          <div className="ai-control-group" role="group" aria-label="Tile size">
            <button type="button" aria-pressed={size === "s"} onClick={() => setSize("s")}>S</button>
            <button type="button" aria-pressed={size === "m"} onClick={() => setSize("m")}>M</button>
            <button type="button" aria-pressed={size === "l"} onClick={() => setSize("l")}>L</button>
          </div>
          <div className="ai-control-group" role="group" aria-label="View">
            <button type="button" aria-pressed={view === "grid"} onClick={() => setView("grid")}>Tiles</button>
            <button type="button" aria-pressed={view === "table"} onClick={() => setView("table")}>Table</button>
          </div>
          <button className="ai-button-secondary" type="button">+ Shelf</button>
        </div>
      </div>

      <div className="ai-library-shelves" role="tablist">
        {shelves.map((s) => {
          const meta = USER_SHELVES.find((u) => u.name === s);
          const icon = meta?.kind === "smart" ? "auto" : meta?.kind === "device" ? "sync" : null;
          return (
            <button
              key={s}
              type="button"
              role="tab"
              className="ai-shelf-chip"
              aria-pressed={shelf === s}
              onClick={() => setShelf(s)}
            >
              {icon && <span className="ai-shelf-chip-icon">{icon}</span>}
              {s}
              {meta && <span style={{ opacity: 0.5, marginLeft: 6 }}>{meta.count}</span>}
            </button>
          );
        })}
      </div>

      {view === "grid" ? (
        <div className="ai-grid" data-size={size}>
          {BOOKS.map((b) => (
            <Tile key={b.id} book={b} theme={theme} showStatus />
          ))}
        </div>
      ) : (
        <table className="ai-table">
          <thead>
            <tr>
              <th>Title</th>
              <th>Year</th>
              <th>Pages</th>
              <th>Status</th>
              <th>Added</th>
            </tr>
          </thead>
          <tbody>
            {BOOKS.map((b) => (
              <tr key={b.id}>
                <td>
                  <span className="ai-mini-cover" style={coverStyle(b, theme)} aria-hidden="true" />
                  <span style={{ verticalAlign: "middle" }}>
                    <span className="title">{b.title}</span>
                    <div className="author">{b.author}</div>
                  </span>
                </td>
                <td>{b.year}</td>
                <td>{b.pages}</td>
                <td>
                  <span className="ai-tile-status-row" style={{ marginTop: 0 }}>
                    <span>
                      <span className={`ai-status-dot ${b.status}`} />
                      {b.status === "in-progress"
                        ? `${Math.round((b.progress ?? 0) * 100)}%`
                        : b.status === "finished" ? "Finished" : "Unread"}
                    </span>
                  </span>
                </td>
                <td>{b.addedDays}d</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

export default function AtelierInk(): ReactElement {
  const [theme, setTheme] = useState<Theme>("dark");
  const [mock, setMock] = useState<Mock>("home");
  const [accent, setAccent] = useState<Accent>("chartreuse");

  return (
    <div className="ai-root" data-theme={theme} data-accent={accent}>
      <header className="ai-topbar">
        <div className="ai-wordmark">
          REVERIE<em>.</em>
        </div>
        <nav className="ai-nav" aria-label="Primary">
          <a href="#" aria-current="page">Library</a>
          <a href="#">Shelves</a>
          <a href="#">Reader</a>
          <a href="#">Stats</a>
        </nav>
        <div className="ai-spacer" />
        <div className="ai-search" aria-label="Search">
          <span aria-hidden="true">⌕</span>
          <span>Title, author, shelf</span>
          <kbd>⌘K</kbd>
        </div>
        <button className="ai-iconbtn" type="button" aria-label="Settings">⚙</button>
        <div className="ai-themetoggle" role="group" aria-label="Theme">
          <button type="button" aria-pressed={theme === "dark"} onClick={() => setTheme("dark")}>Dark</button>
          <button type="button" aria-pressed={theme === "light"} onClick={() => setTheme("light")}>Light</button>
        </div>
        <div className="ai-avatar" aria-label="User menu">J</div>
      </header>

      <div className="ai-mocktabs" role="tablist" aria-label="Mock screen">
        <button type="button" role="tab" className="ai-mocktab" aria-pressed={mock === "home"} onClick={() => setMock("home")}>
          <span className="num">i</span> Home
        </button>
        <button type="button" role="tab" className="ai-mocktab" aria-pressed={mock === "detail"} onClick={() => setMock("detail")}>
          <span className="num">ii</span> Book detail
        </button>
        <button type="button" role="tab" className="ai-mocktab" aria-pressed={mock === "library"} onClick={() => setMock("library")}>
          <span className="num">iii</span> Library
        </button>
        <div className="ai-spacer" />
        <div className="ai-accent-picker" role="group" aria-label="Accent">
          <button
            type="button"
            className="ai-accent-swatch ai-accent-swatch-chartreuse"
            aria-pressed={accent === "chartreuse"}
            aria-label="Chartreuse accent"
            onClick={() => setAccent("chartreuse")}
          />
          <button
            type="button"
            className="ai-accent-swatch ai-accent-swatch-persimmon"
            aria-pressed={accent === "persimmon"}
            aria-label="Persimmon accent"
            onClick={() => setAccent("persimmon")}
          />
          <button
            type="button"
            className="ai-accent-swatch ai-accent-swatch-carmine"
            aria-pressed={accent === "carmine"}
            aria-label="Carmine accent"
            onClick={() => setAccent("carmine")}
          />
        </div>
        <Link
          to="/design/explore"
          style={{
            color: "var(--ai-fg-faint)",
            fontSize: "var(--ai-type-small)",
            letterSpacing: "var(--ai-tracking-caps)",
            textTransform: "uppercase",
            textDecoration: "none",
            alignSelf: "center",
            marginRight: "var(--ai-space-7)",
            fontWeight: 500,
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
