import { describe, it, expect } from "vite-plus/test";
import { render } from "vitest-browser-react";
import { page, server } from "vite-plus/test/browser";
import { Lockup } from "@/components/Lockup";

describe("Lockup", () => {
  it("renders the wordmark text", async () => {
    await render(<Lockup />);
    await expect.element(page.getByText("Reverie")).toBeInTheDocument();
  });

  it("exposes the lockup as a single image to assistive tech", async () => {
    await render(<Lockup />);
    const lockup = page.getByRole("img", { name: "Reverie" });
    await expect.element(lockup).toBeInTheDocument();
  });

  it("hides the canonical glyph asset from assistive tech (the parent has the label)", async () => {
    const { container } = await render(<Lockup />);
    const glyph = container.querySelector("svg");
    expect(glyph).not.toBeNull();
    await expect.element(glyph).toHaveAttribute("aria-hidden", "true");
  });

  it.each([
    { size: 17, asset: "slot-favicon.svg" },
    { size: 18, asset: "slot.svg" },
  ])(
    "inlines the exact canonical knockout at the $size px variant boundary",
    async ({ size, asset }) => {
      const artwork = await server.commands.readFile(`public/brand/glyph/${asset}`);
      const { container } = await render(<Lockup size={size} />);
      const paths = container.querySelectorAll("svg path");
      const canonical = new DOMParser().parseFromString(artwork, "image/svg+xml");
      const canonicalPath = canonical.querySelector("path");
      expect(paths).toHaveLength(1);
      const path = container.querySelector("svg path");
      if (canonicalPath === null || path === null) throw new Error("Missing glyph path");
      const attributes = (element: Element): Record<string, string> =>
        Object.fromEntries(Array.from(element.attributes, ({ name, value }) => [name, value]));
      expect(attributes(path)).toEqual(attributes(canonicalPath));
    },
  );

  it("uses cream wordmark on dark theme (default)", async () => {
    await render(<Lockup />);
    const word = page.getByText("Reverie");
    await expect.element(word).toHaveStyle({ color: "rgb(232, 224, 208)" }); // #E8E0D0
  });

  it("uses ink wordmark on light theme", async () => {
    await render(<Lockup theme="light" />);
    const word = page.getByText("Reverie");
    await expect.element(word).toHaveStyle({ color: "rgb(14, 13, 10)" }); // #0E0D0A
  });

  it("sizes the glyph and wordmark gap from the wordmark type size", async () => {
    await render(<Lockup size={40} />);
    await expect
      .element(page.getByRole("img", { name: "Reverie" }))
      .toHaveStyle({ fontSize: "40px", gap: "0.48em" });
    const word = page.getByText("Reverie");
    expect(word.element().style.paddingLeft).toBe("0.32em");
    expect(word.element().style.letterSpacing).toBe("0.32em");
  });

  it.each([
    { size: 13, blockSize: 18.2 },
    { size: 20, blockSize: 28 },
    { size: 32, blockSize: 44.8 },
  ])("frames the visible canonical block at $size px type", async ({ size, blockSize }) => {
    const { container } = await render(<Lockup size={size} />);
    const glyph = container.querySelector("svg");
    expect(glyph).not.toBeNull();
    expect(Number(glyph?.getAttribute("width"))).toBeCloseTo(blockSize);
    expect(Number(glyph?.getAttribute("height"))).toBeCloseTo(blockSize);
    await expect.element(glyph).toHaveAttribute("viewBox", "4 4 24 24");
  });

  it("forwards className to the lockup element", async () => {
    await render(<Lockup className="custom-class" />);
    await expect.element(page.getByRole("img", { name: "Reverie" })).toHaveClass("custom-class");
  });
});
