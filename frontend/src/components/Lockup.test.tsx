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
    const glyph = container.querySelector("img");
    expect(glyph).not.toBeNull();
    expect(glyph).toHaveAttribute("aria-hidden", "true");
  });

  it("uses the canonical standard Slot asset", () => {
    const { container } = render(<Lockup />);
    expect(container.querySelector("img")).toHaveAttribute("src", "/brand/glyph/slot.svg");
  });

  it("uses the canonical thick-slot asset when the glyph renders below 24px", () => {
    const { container } = render(<Lockup size={13} />);
    expect(container.querySelector("img")).toHaveAttribute("src", "/brand/glyph/slot-favicon.svg");
  });

  it("switches variant on the rendered glyph size, not the wordmark size", () => {
    const thick = render(<Lockup size={17} />);
    expect(thick.container.querySelector("img")).toHaveAttribute(
      "src",
      "/brand/glyph/slot-favicon.svg",
    );

    const standard = render(<Lockup size={18} />);
    expect(standard.container.querySelector("img")).toHaveAttribute("src", "/brand/glyph/slot.svg");
  });

  it("ships knockout artwork for both glyph sources", () => {
    for (const svg of [slotSvg, slotFaviconSvg]) {
      expect(svg).toContain('fill-rule="evenodd"');
      expect(svg).toContain('fill="#C9A961"');
      expect(svg).not.toContain("<rect");
    }
  });

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
    expect(container.querySelector("img")?.parentElement).toHaveStyle({
      width: "56px",
      height: "56px",
    });
  });

  it.each([
    { size: 13, blockSize: 18.2, artwork: slotFaviconSvg },
    { size: 20, blockSize: 28, artwork: slotSvg },
    { size: 32, blockSize: 44.8, artwork: slotSvg },
  ])("frames the visible canonical block at $size px type", ({ size, blockSize, artwork }) => {
    const { container } = render(<Lockup size={size} />);
    const image = container.querySelector("img");
    expect(image).not.toBeNull();
    if (image === null) throw new Error("Missing glyph image");
    const frame = image.parentElement;
    expect(frame).toHaveStyle({
      width: `${String(blockSize)}px`,
      height: `${String(blockSize)}px`,
    });
    expect(frame).toHaveClass(
      "inline-flex",
      "flex-none",
      "items-center",
      "justify-center",
      "overflow-hidden",
    );
    const svg = new DOMParser().parseFromString(artwork, "image/svg+xml");
    const gridWidth = Number(svg.documentElement.getAttribute("viewBox")?.split(" ")[2]);
    const blockWidth = Number(
      svg
        .querySelector("path")
        ?.getAttribute("d")
        ?.match(/h(\d+)/)?.[1],
    );
    expect(gridWidth).toBeGreaterThan(blockWidth);
    expect(blockWidth).toBeGreaterThan(0);
    expect((Number.parseFloat(image.style.width) / 100) * (blockWidth / gridWidth)).toBeCloseTo(1);
    expect(image.style.height).toBe(image.style.width);
    expect(image).toHaveStyle({ flex: "none" });
    expect(image).toHaveClass("max-w-none");
  });

  it("forwards className to the lockup element", () => {
    const { container } = render(<Lockup className="custom-class" />);
    expect(container.firstElementChild).toHaveClass("custom-class");
  });
});
