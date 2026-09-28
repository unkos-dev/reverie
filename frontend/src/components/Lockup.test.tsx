import { describe, it, expect } from "vite-plus/test";
import { render, screen } from "@testing-library/react";
import { Lockup } from "./Lockup";
import slotFaviconSvg from "../../public/brand/glyph/slot-favicon.svg?raw";
import slotSvg from "../../public/brand/glyph/slot.svg?raw";

describe("Lockup", () => {
  it("renders the wordmark text", () => {
    render(<Lockup />);
    expect(screen.getByText("Reverie")).toBeInTheDocument();
  });

  it("exposes the lockup as a single image to assistive tech", () => {
    render(<Lockup />);
    const lockup = screen.getByRole("img", { name: "Reverie" });
    expect(lockup).toBeInTheDocument();
  });

  it("hides the canonical glyph asset from assistive tech (the parent has the label)", () => {
    const { container } = render(<Lockup />);
    const glyph = container.querySelector("svg");
    expect(glyph).not.toBeNull();
    expect(glyph).toHaveAttribute("aria-hidden", "true");
  });

  it.each([
    { size: 17, artwork: slotFaviconSvg },
    { size: 18, artwork: slotSvg },
  ])(
    "inlines the exact canonical knockout at the $size px variant boundary",
    ({ size, artwork }) => {
      const { container } = render(<Lockup size={size} />);
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

  it("uses cream wordmark on dark theme (default)", () => {
    render(<Lockup />);
    const word = screen.getByText("Reverie");
    expect(word).toHaveStyle({ color: "rgb(232, 224, 208)" }); // #E8E0D0
  });

  it("uses ink wordmark on light theme", () => {
    render(<Lockup theme="light" />);
    const word = screen.getByText("Reverie");
    expect(word).toHaveStyle({ color: "rgb(14, 13, 10)" }); // #0E0D0A
  });

  it("sizes the glyph and wordmark gap from the wordmark type size", () => {
    const { container } = render(<Lockup size={40} />);
    expect(container.firstElementChild).toHaveStyle({ fontSize: "40px", gap: "0.48em" });
    const word = screen.getByText("Reverie");
    expect(word.style.paddingLeft).toBe("0.32em");
    expect(word.style.letterSpacing).toBe("0.32em");
  });

  it.each([
    { size: 13, blockSize: 18.2 },
    { size: 20, blockSize: 28 },
    { size: 32, blockSize: 44.8 },
  ])("frames the visible canonical block at $size px type", ({ size, blockSize }) => {
    const { container } = render(<Lockup size={size} />);
    const glyph = container.querySelector("svg");
    expect(glyph).not.toBeNull();
    expect(Number(glyph?.getAttribute("width"))).toBeCloseTo(blockSize);
    expect(Number(glyph?.getAttribute("height"))).toBeCloseTo(blockSize);
    expect(glyph).toHaveAttribute("viewBox", "4 4 24 24");
  });

  it("forwards className to the lockup element", () => {
    const { container } = render(<Lockup className="custom-class" />);
    expect(container.firstElementChild).toHaveClass("custom-class");
  });
});
