import { render } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { CoverArtwork } from "./CoverArtwork";

function coverEl(container: HTMLElement): HTMLElement {
  const el = container.querySelector("[data-cloth]");
  if (!(el instanceof HTMLElement)) throw new Error("cover root not rendered");
  return el;
}

describe("CoverArtwork — determinism", () => {
  test("same book id renders the same cloth and texture every time", () => {
    const a = render(<CoverArtwork bookId="abc-123" title="Stoner" authors={["John Williams"]} />);
    const b = render(<CoverArtwork bookId="abc-123" title="Stoner" authors={["John Williams"]} />);
    expect(coverEl(a.container).dataset.cloth).toBe(coverEl(b.container).dataset.cloth);
    expect(coverEl(a.container).dataset.texture).toBe(coverEl(b.container).dataset.texture);
  });

  test("all six cloth tones and every texture are reachable", () => {
    const tones = new Set<string>();
    const textures = new Set<string>();
    for (let i = 0; i < 200; i++) {
      const { container, unmount } = render(
        <CoverArtwork bookId={`book-${String(i)}`} title="T" authors={["A"]} />,
      );
      const el = coverEl(container);
      if (el.dataset.cloth !== undefined) tones.add(el.dataset.cloth);
      if (el.dataset.texture !== undefined) textures.add(el.dataset.texture);
      unmount();
    }
    expect(tones).toEqual(
      new Set(["bordeaux", "oxblood", "midnight", "charcoal", "sepia", "terracotta"]),
    );
    expect(textures).toEqual(new Set(["linen", "buckram", "marbled", "plain", "leather"]));
  });
});

describe("CoverArtwork — content", () => {
  test("carries the full title in <title> and the upper-cased author", () => {
    const { container, getByText } = render(
      <CoverArtwork bookId="x" title="The Dispossessed" authors={["Ursula K. Le Guin", "Other"]} />,
    );
    // The full title lives in the SVG <title> element even though the
    // visible gilt text is wrapped across lines.
    expect(container.querySelector("title")?.textContent).toBe("The Dispossessed");
    expect(getByText("URSULA K. LE GUIN")).toBeInTheDocument();
  });

  test("tolerates an empty authors list", () => {
    const { container } = render(<CoverArtwork bookId="x" title="Anon" authors={[]} />);
    expect(container.querySelector("title")?.textContent).toBe("Anon");
  });

  test("keeps the full title even when it is far too long to render", () => {
    const long = "An Extremely Long Title That Goes On And On And Should Be Wrapped".repeat(3);
    const { container } = render(<CoverArtwork bookId="x" title={long} authors={["A"]} />);
    // Visible gilt is capped to three lines; the <title> keeps the whole
    // string so nothing is lost to tooling / assistive tech.
    expect(container.querySelector("title")?.textContent).toBe(long);
  });

  test("applies the light-theme pedestal treatment", () => {
    const { container } = render(<CoverArtwork bookId="x" title="T" authors={["A"]} />);
    // Pedestal: a shadow on light (dark cloth on parchment canvas),
    // neutralized on dark where contrast is inherent.
    expect(coverEl(container).className).toMatch(/shadow/);
  });
});
