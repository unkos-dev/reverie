import { useState, type ReactElement, type CSSProperties } from "react";
import { Link } from "react-router";
import "../../../design/explore/signal/tokens.css";
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

function coverStyle(book: Book, theme: Theme): CSSProperties {
  const hue = bookHue(book.id);
  const tier = bookTier(book.id);
  if (theme === "dark") {
    const top = `hsl(${hue}, 14%, ${28 + tier * 2}%)`;
    const bot = `hsl(${hue}, 10%, ${14 + tier}%)`;
    return {
      background: `linear-gradient(180deg, ${top} 0%, ${bot} 100%)`,
      color: "#F5F6F8",
    };
  }
  const top = `hsl(${hue}, 14%, ${88 - tier}%)`;
  const bot = `hsl(${hue}, 12%, ${72 - tier * 2}%)`;
  return {
    background: `linear-gradient(180deg, ${top} 0%, ${bot} 100%)`,
    color: "#0A0B10",
  };
}

function accentStripeWidth(book: Book): string {
  const seed = (bookHue(book.id) % 70) + 12;
  return `${seed}%`;
}

interface CoverProps {
  book: Book;
  theme: Theme;
}

function Cover({ book, theme }: CoverProps): ReactElement {
  const num = book.id.replace("b", "");
  return (
    <div className="sig-tile-cover">
      <div className="sig-tile-cover-inner" style={coverStyle(book, theme)}>
        <div className="sig-tile-cover-num">№ {num}</div>
        <div>
          <div className="sig-tile-cover-title">{book.title}</div>
          <div className="sig-tile-cover-author">{book.author}</div>
        </div>
      </div>
      <div className="sig-tile-cover-stripe" style={{ width: accentStripeWidth(book) }} />
    </div>
  );
}

interface TileProps {
  book: Book;
  theme: Theme;
  size?: "default" | "lg" | "sm";
}

function Tile({ book, theme, size = "default" }: TileProps): ReactElement {
  const cls = size === "lg" ? "sig-tile sig-tile-lg" : size === "sm" ? "sig-tile sig-tile-sm" : "sig-tile";
  return (
    <article className={cls}>
      <Cover book={book} theme={theme} />
      <div className="sig-tile-meta">
        <div className="sig-tile-meta-text">
          <h4 className="sig-tile-title">{book.title}</h4>
          <div className="sig-tile-author">{book.author}</div>
        </div>
        <div className="sig-tile-year">{book.year}</div>
      </div>
    </article>
  );
}

function Home({ theme }: { theme: Theme }): ReactElement {
  const featured = SHELVES.inProgress[0] ?? BOOKS[0];
  return (
    <>
      <section className="sig-hero">
        <div className="sig-hero-grid">
          <div>
            <div className="sig-eyebrow">Wed · Apr 25 · 21:14</div>
            <h1 className="sig-hero-h">
              {STATS.totalBooks.toLocaleString()}
              <sup>library</sup>
            </h1>
            <div className="sig-hero-action">
              <button type="button" className="sig-button accent">
                Resume “{featured.title}”
                <span className="arrow" aria-hidden="true">→</span>
              </button>
              <button type="button" className="sig-button ghost">Browse</button>
            </div>
          </div>
          <div className="sig-hero-stats">
            <div>
              <div className="sig-hero-stat-num">{STATS.read}</div>
              <div className="sig-hero-stat-label">Read all-time</div>
            </div>
            <div>
              <div className="sig-hero-stat-num accent">{STATS.inProgress}</div>
              <div className="sig-hero-stat-label">In progress</div>
            </div>
            <div>
              <div className="sig-hero-stat-num">{STATS.finishedThisYear}</div>
              <div className="sig-hero-stat-label">Finished · 2026</div>
            </div>
            <div>
              <div className="sig-hero-stat-num">{STATS.hoursThisYear}h</div>
              <div className="sig-hero-stat-label">Read · 2026</div>
            </div>
          </div>
        </div>
      </section>

      <section className="sig-shelf">
        <div className="sig-shelf-head">
          <h2 className="sig-shelf-title">In progress</h2>
          <div className="sig-shelf-meta">{SHELVES.inProgress.length} active</div>
        </div>
        <div className="sig-carousel">
          {SHELVES.inProgress.map((b) => (
            <Tile key={b.id} book={b} theme={theme} size="lg" />
          ))}
        </div>
      </section>

      <section className="sig-shelf">
        <div className="sig-shelf-head">
          <h2 className="sig-shelf-title">Recently added</h2>
          <div className="sig-shelf-meta">
            <span className="sig-tag neutral">last 7 days</span>
            <a href="#">view all →</a>
          </div>
        </div>
        <div className="sig-carousel">
          {SHELVES.recentlyAdded.map((b) => (
            <Tile key={b.id} book={b} theme={theme} />
          ))}
        </div>
      </section>

      <section className="sig-shelf">
        <div className="sig-shelf-head">
          <h2 className="sig-shelf-title">Forgotten favourites</h2>
          <div className="sig-shelf-meta">
            <span className="sig-tag">discovery</span>
          </div>
        </div>
        <div className="sig-carousel">
          {SHELVES.forgotten.map((b) => (
            <Tile key={b.id} book={b} theme={theme} />
          ))}
        </div>
      </section>

      <section className="sig-shelf">
        <div className="sig-shelf-head">
          <h2 className="sig-shelf-title">Lantern Cycle · incomplete</h2>
          <div className="sig-shelf-meta">
            <span className="sig-tag">smart shelf</span>
          </div>
        </div>
        <div className="sig-carousel">
          {SHELVES.byYusra.map((b) => (
            <Tile key={b.id} book={b} theme={theme} />
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
    <article className="sig-detail">
      <div className="sig-detail-grid">
        <aside>
          <div className="sig-detail-cover">
            <div className="sig-tile-cover-inner" style={coverStyle(book, theme)}>
              <div className="sig-tile-cover-num">№ {book.id.replace("b", "")}</div>
              <div>
                <div className="sig-tile-cover-title">{book.title}</div>
                <div className="sig-tile-cover-author">{book.author}</div>
              </div>
            </div>
            <div className="sig-tile-cover-stripe" style={{ width: accentStripeWidth(book) }} />
          </div>
          <dl className="sig-detail-aside">
            <div className="sig-aside-row"><dt>format</dt><dd>{book.format.toUpperCase()}</dd></div>
            <div className="sig-aside-row"><dt>pages</dt><dd>{book.pages}</dd></div>
            <div className="sig-aside-row"><dt>year</dt><dd>{book.year}</dd></div>
            <div className="sig-aside-row"><dt>added</dt><dd>{book.addedDays}d ago</dd></div>
            <div className="sig-aside-row">
              <dt>status</dt>
              <dd className="accent">
                {book.status === "in-progress"
                  ? `${Math.round((book.progress ?? 0) * 100)}% read`
                  : book.status === "finished" ? "finished" : "unread"}
              </dd>
            </div>
          </dl>
        </aside>
        <div>
          <div className="sig-detail-eyebrow">
            <span style={{ color: "var(--sig-accent)" }}>●</span>
            Reverie / detail / {book.id}
          </div>
          <h1 className="sig-detail-title">{book.title}</h1>
          <p className="sig-detail-byline">by <span>{book.author}</span></p>
          <div className="sig-detail-actions">
            <button className="sig-button accent" type="button">
              Resume at 78%
              <span className="arrow" aria-hidden="true">→</span>
            </button>
            <button className="sig-button" type="button">Add to shelf</button>
            <button className="sig-button ghost" type="button">Send to Kobo</button>
            <button className="sig-button ghost" type="button">Edit metadata</button>
          </div>
          <div className="sig-detail-summary">
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
          <section className="sig-detail-section">
            <h3>More by {book.author}</h3>
            <div className="sig-detail-row">
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
    <div className="sig-library">
      <div className="sig-library-head">
        <div>
          <div className="sig-eyebrow">{BOOKS.length} of {STATS.totalBooks.toLocaleString()}</div>
          <h1 className="sig-library-title">
            Library<span className="accent">.</span>
          </h1>
        </div>
        <div className="sig-library-controls">
          <div className="sig-control-group" role="group" aria-label="Tile size">
            <button type="button" aria-pressed={size === "s"} onClick={() => setSize("s")}>S</button>
            <button type="button" aria-pressed={size === "m"} onClick={() => setSize("m")}>M</button>
            <button type="button" aria-pressed={size === "l"} onClick={() => setSize("l")}>L</button>
          </div>
          <div className="sig-control-group" role="group" aria-label="View">
            <button type="button" aria-pressed={view === "grid"} onClick={() => setView("grid")}>tiles</button>
            <button type="button" aria-pressed={view === "table"} onClick={() => setView("table")}>table</button>
          </div>
          <button className="sig-button" type="button">+ shelf</button>
        </div>
      </div>

      <div className="sig-library-shelves" role="tablist">
        {shelves.map((s) => {
          const meta = USER_SHELVES.find((u) => u.name === s);
          const icon = meta?.kind === "smart" ? "auto" : meta?.kind === "device" ? "sync" : null;
          return (
            <button
              key={s}
              type="button"
              role="tab"
              className="sig-shelf-chip"
              aria-pressed={shelf === s}
              onClick={() => setShelf(s)}
            >
              {icon && <span className="sig-shelf-chip-icon">{icon}</span>}
              {s}
              {meta && <span style={{ opacity: 0.5, marginLeft: 6 }}>{meta.count}</span>}
            </button>
          );
        })}
      </div>

      {view === "grid" ? (
        <div className="sig-grid" data-size={size}>
          {BOOKS.map((b) => (
            <Tile key={b.id} book={b} theme={theme} />
          ))}
        </div>
      ) : (
        <table className="sig-table">
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
                  <span className="sig-mini-cover" style={coverStyle(b, theme)} aria-hidden="true" />
                  <span style={{ verticalAlign: "middle" }}>
                    <span className="title">{b.title}</span>
                    <div className="author">{b.author}</div>
                  </span>
                </td>
                <td>{b.year}</td>
                <td>{b.pages}</td>
                <td>
                  <span className="sig-status">
                    <span className={`sig-status-dot ${b.status}`} />
                    {b.status === "in-progress"
                      ? `${Math.round((b.progress ?? 0) * 100)}%`
                      : b.status === "finished" ? "finished" : "unread"}
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

export default function Signal(): ReactElement {
  const [theme, setTheme] = useState<Theme>("dark");
  const [mock, setMock] = useState<Mock>("home");

  return (
    <div className="sig-root" data-theme={theme}>
      <header className="sig-topbar">
        <div className="sig-wordmark">REVERIE</div>
        <nav className="sig-nav" aria-label="Primary">
          <a href="#" aria-current="page">library</a>
          <a href="#">shelves</a>
          <a href="#">reader</a>
          <a href="#">stats</a>
        </nav>
        <div className="sig-spacer" />
        <div className="sig-search" aria-label="Search">
          <span aria-hidden="true">/</span>
          <span>search</span>
          <kbd>⌘K</kbd>
        </div>
        <button className="sig-iconbtn" type="button" aria-label="Settings">⚙</button>
        <div className="sig-themetoggle" role="group" aria-label="Theme">
          <button type="button" aria-pressed={theme === "dark"} onClick={() => setTheme("dark")}>dark</button>
          <button type="button" aria-pressed={theme === "light"} onClick={() => setTheme("light")}>light</button>
        </div>
        <div className="sig-avatar" aria-label="User menu">JU</div>
      </header>

      <div className="sig-mocktabs" role="tablist" aria-label="Mock screen">
        <button type="button" role="tab" className="sig-mocktab" aria-pressed={mock === "home"} onClick={() => setMock("home")}>
          <span className="num">01</span> home
        </button>
        <button type="button" role="tab" className="sig-mocktab" aria-pressed={mock === "detail"} onClick={() => setMock("detail")}>
          <span className="num">02</span> book detail
        </button>
        <button type="button" role="tab" className="sig-mocktab" aria-pressed={mock === "library"} onClick={() => setMock("library")}>
          <span className="num">03</span> library
        </button>
        <div className="sig-spacer" />
        <Link
          to="/design/explore"
          style={{
            color: "var(--sig-fg-faint)",
            fontFamily: "var(--sig-font-mono)",
            fontSize: "var(--sig-type-small)",
            letterSpacing: "var(--sig-tracking-mono)",
            textDecoration: "none",
            alignSelf: "center",
            marginRight: "var(--sig-space-7)",
          }}
        >
          ← all directions
        </Link>
      </div>

      {mock === "home" && <Home theme={theme} />}
      {mock === "detail" && <Detail theme={theme} />}
      {mock === "library" && <Library theme={theme} />}
    </div>
  );
}
