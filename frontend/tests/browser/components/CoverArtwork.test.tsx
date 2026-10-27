import { render } from "vitest-browser-react";
import { describe, expect, test } from "vite-plus/test";

import { CoverArtwork } from "@/components/CoverArtwork";

function coverEl(container: HTMLElement): HTMLElement {
  const el = container.querySelector("[data-cloth]");
  if (!(el instanceof HTMLElement)) throw new Error("cover root not rendered");
  return el;
}

describe("CoverArtwork — determinism", () => {
  test("same book id renders the same cloth and texture every time", async () => {
    const a = await render(
      <CoverArtwork bookId="abc-123" title="Stoner" authors={["John Williams"]} />,
    );
    const b = await render(
      <CoverArtwork bookId="abc-123" title="Stoner" authors={["John Williams"]} />,
    );
    expect(coverEl(a.container).dataset.cloth).toBe(coverEl(b.container).dataset.cloth);
    expect(coverEl(a.container).dataset.texture).toBe(coverEl(b.container).dataset.texture);
  });

  test("all six cloth tones and every texture are reachable", async () => {
    const tones = new Set<string>();
    const textures = new Set<string>();
    for (let i = 0; i < 200; i++) {
      const { container, unmount } = await render(
        <CoverArtwork bookId={`book-${String(i)}`} title="T" authors={["A"]} />,
      );
      const el = coverEl(container);
      if (el.dataset.cloth !== undefined) tones.add(el.dataset.cloth);
      if (el.dataset.texture !== undefined) textures.add(el.dataset.texture);
      await unmount();
    }
    expect(tones).toEqual(
      new Set(["bordeaux", "oxblood", "midnight", "charcoal", "sepia", "terracotta"]),
    );
    expect(textures).toEqual(new Set(["linen", "buckram", "marbled", "plain", "leather"]));
  });
});

describe("CoverArtwork — content", () => {
  test("carries the full title in <title> and the upper-cased author", async () => {
    const { container, getByText } = await render(
      <CoverArtwork bookId="x" title="The Dispossessed" authors={["Ursula K. Le Guin", "Other"]} />,
    );
    // The full title lives in the SVG <title> element even though the
    // visible gilt text is wrapped across lines.
    expect(container.querySelector("title")?.textContent).toBe("The Dispossessed");
    await expect.element(getByText("URSULA K. LE GUIN")).toBeInTheDocument();
  });

  test("tolerates an empty authors list", async () => {
    const { container } = await render(<CoverArtwork bookId="x" title="Anon" authors={[]} />);
    expect(container.querySelector("title")?.textContent).toBe("Anon");
  });

  test("keeps the full title even when it is far too long to render", async () => {
    const long = "An Extremely Long Title That Goes On And On And Should Be Wrapped".repeat(3);
    const { container } = await render(<CoverArtwork bookId="x" title={long} authors={["A"]} />);
    // Visible gilt is capped to three lines; the <title> keeps the whole
    // string so nothing is lost to tooling / assistive tech.
    expect(container.querySelector("title")?.textContent).toBe(long);
  });

  test("applies the light-theme pedestal treatment", async () => {
    const { container } = await render(<CoverArtwork bookId="x" title="T" authors={["A"]} />);
    // Pedestal: a shadow on light (dark cloth on parchment canvas),
    // neutralized on dark where contrast is inherent.
    expect(coverEl(container).className).toMatch(/shadow/);
  });
});
