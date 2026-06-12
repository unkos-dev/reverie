import { render } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { CoverArtwork } from "./CoverArtwork";

function coverEl(container: HTMLElement): HTMLElement {
  const el = container.querySelector("[data-layout]");
  if (!(el instanceof HTMLElement)) throw new Error("cover root not rendered");
  return el;
}

describe("CoverArtwork — determinism", () => {
  test("same book id renders the same layout and colorway every time", () => {
    const a = render(<CoverArtwork bookId="abc-123" title="Stoner" authors={["John Williams"]} />);
    const b = render(<CoverArtwork bookId="abc-123" title="Stoner" authors={["John Williams"]} />);
    expect(coverEl(a.container).dataset.layout).toBe(coverEl(b.container).dataset.layout);
    expect(coverEl(a.container).dataset.colorway).toBe(coverEl(b.container).dataset.colorway);
  });

  test("all five layouts and four colorways are reachable", () => {
    const layouts = new Set<string>();
    const colorways = new Set<string>();
    for (let i = 0; i < 60; i++) {
      const { container, unmount } = render(
        <CoverArtwork bookId={`book-${String(i)}`} title="T" authors={["A"]} />,
      );
      const el = coverEl(container);
      if (el.dataset.layout !== undefined) layouts.add(el.dataset.layout);
      if (el.dataset.colorway !== undefined) colorways.add(el.dataset.colorway);
      unmount();
    }
    expect(layouts).toEqual(new Set(["standard", "monogram", "vertical", "framed", "band"]));
    expect(colorways).toEqual(new Set(["ink", "cream", "parchment", "gold"]));
  });
});

describe("CoverArtwork — content", () => {
  test("renders title and first author", () => {
    const { getByText } = render(
      <CoverArtwork bookId="x" title="The Dispossessed" authors={["Ursula K. Le Guin", "Other"]} />,
    );
    expect(getByText("The Dispossessed")).toBeInTheDocument();
    expect(getByText("Ursula K. Le Guin")).toBeInTheDocument();
  });

  test("tolerates an empty authors list", () => {
    const { getByText } = render(<CoverArtwork bookId="x" title="Anon" authors={[]} />);
    expect(getByText("Anon")).toBeInTheDocument();
  });

  test("long titles carry a clamp so they cannot overflow the cover", () => {
    const long = "An Extremely Long Title That Goes On And On And Should Be Clamped".repeat(3);
    const { getByText } = render(<CoverArtwork bookId="x" title={long} authors={["A"]} />);
    expect(getByText(long).className).toMatch(/line-clamp/);
  });

  test("applies the light-theme pedestal treatment", () => {
    const { container } = render(<CoverArtwork bookId="x" title="T" authors={["A"]} />);
    // Pedestal: border + shadow on light (parchment cover on parchment
    // canvas), neutralized on dark where contrast is inherent.
    expect(coverEl(container).className).toMatch(/border/);
  });
});
