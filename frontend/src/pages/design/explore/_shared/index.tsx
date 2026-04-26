import { Link } from "react-router";

const DIRECTIONS = [
  {
    slug: "midnight-gold",
    name: "Midnight Gold",
    summary:
      "Charcoal canvas, warm muted surfaces, brushed-gold accent. Modern serif on book content (Spectral-class), slow-decisive motion.",
    accent: "#B3945B",
    bg: "#15140F",
    fg: "#EAE2D2",
  },
  {
    slug: "signal",
    name: "Signal",
    summary:
      "Sans-only exception. High-contrast monochrome with a single saturated accent (burnt orange). Bricolage Grotesque + Geist class.",
    accent: "#E0664A",
    bg: "#0A0B10",
    fg: "#F5F6F8",
  },
  {
    slug: "atelier-ink",
    name: "Atelier Ink",
    summary:
      "Cool ink-noir + parchment, electric chartreuse accent. Sectra-class inktrap serif, fast-decisive motion. Editorial-modern register.",
    accent: "#C5E84A",
    bg: "#0E1015",
    fg: "#EFEAE0",
  },
];

export default function ExploreIndex() {
  return (
    <div
      style={{
        minHeight: "100vh",
        background: "#0F0F12",
        color: "#E8E8EA",
        fontFamily: "system-ui, -apple-system, sans-serif",
        padding: "64px 48px",
      }}
    >
      <header style={{ maxWidth: 880, margin: "0 auto 48px" }}>
        <div style={{ fontSize: 11, letterSpacing: "0.18em", textTransform: "uppercase", color: "#7A7A82" }}>
          Reverie · Design system · D2 visual exploration
        </div>
        <h1 style={{ fontSize: 36, fontWeight: 600, margin: "12px 0 8px", letterSpacing: "-0.01em" }}>
          Three directions
        </h1>
        <p style={{ fontSize: 15, lineHeight: 1.6, color: "#A8A8B0", maxWidth: 640 }}>
          Each direction renders the same three mock screens — home dashboard, book detail, library
          full-grid — themed in Dark and Light. Use the in-page toggles to compare. This route tree is
          throwaway and gets pruned at the start of D3.
        </p>
      </header>
      <ul style={{ maxWidth: 880, margin: "0 auto", listStyle: "none", padding: 0, display: "grid", gap: 16 }}>
        {DIRECTIONS.map((d) => (
          <li key={d.slug}>
            <Link
              to={`/design/explore/${d.slug}`}
              style={{
                display: "grid",
                gridTemplateColumns: "120px 1fr",
                gap: 24,
                padding: 20,
                background: "#16161B",
                border: "1px solid #24242C",
                borderRadius: 12,
                textDecoration: "none",
                color: "inherit",
              }}
            >
              <div
                style={{
                  background: d.bg,
                  color: d.fg,
                  display: "grid",
                  placeItems: "center",
                  borderRadius: 8,
                  position: "relative",
                  overflow: "hidden",
                }}
              >
                <span style={{ fontSize: 36, fontWeight: 300, letterSpacing: "-0.02em" }}>R</span>
                <span
                  style={{
                    position: "absolute",
                    bottom: 8,
                    left: 8,
                    width: 18,
                    height: 4,
                    background: d.accent,
                    borderRadius: 2,
                  }}
                />
              </div>
              <div>
                <div style={{ fontSize: 18, fontWeight: 600, marginBottom: 6 }}>{d.name}</div>
                <div style={{ fontSize: 14, lineHeight: 1.55, color: "#9A9AA4" }}>{d.summary}</div>
                <div
                  style={{
                    marginTop: 10,
                    fontSize: 11,
                    letterSpacing: "0.12em",
                    textTransform: "uppercase",
                    color: d.accent,
                  }}
                >
                  Open →
                </div>
              </div>
            </Link>
          </li>
        ))}
      </ul>
      <footer style={{ maxWidth: 880, margin: "48px auto 0", fontSize: 12, color: "#5C5C66", lineHeight: 1.6 }}>
        Source: <code style={{ color: "#9A9AA4" }}>frontend/src/pages/design/explore/*</code> ·{" "}
        Tokens: <code style={{ color: "#9A9AA4" }}>frontend/src/design/explore/[name]/tokens.css</code>
      </footer>
    </div>
  );
}
