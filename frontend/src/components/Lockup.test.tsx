import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Lockup } from "./Lockup";

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

  it("hides the inline glyph SVG from assistive tech (the parent has the label)", () => {
    const { container } = render(<Lockup />);
    const glyph = container.querySelector("svg");
    expect(glyph).not.toBeNull();
    expect(glyph).toHaveAttribute("aria-hidden", "true");
  });

  it("renders the locked Slot construction inside the inline glyph", () => {
    const { container } = render(<Lockup />);
    const glyph = container.querySelector("svg");
    expect(glyph).toHaveAttribute("viewBox", "0 0 32 32");
    const rects = glyph?.querySelectorAll("rect");
    expect(rects?.length).toBe(2);
    expect(rects?.[0]).toHaveAttribute("x", "4");
    expect(rects?.[0]).toHaveAttribute("y", "4");
    expect(rects?.[0]).toHaveAttribute("width", "24");
    expect(rects?.[0]).toHaveAttribute("height", "24");
    expect(rects?.[0]).toHaveAttribute("fill", "#C9A961");
    expect(rects?.[1]).toHaveAttribute("x", "8");
    expect(rects?.[1]).toHaveAttribute("y", "17");
    expect(rects?.[1]).toHaveAttribute("width", "16");
    expect(rects?.[1]).toHaveAttribute("height", "2");
    expect(rects?.[1]).toHaveAttribute("fill", "#0E0D0A");
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

  it("scales glyph to 0.95 × size", () => {
    const { container } = render(<Lockup size={40} />);
    const glyph = container.querySelector("svg");
    expect(glyph).toHaveAttribute("width", "38"); // 40 * 0.95
    expect(glyph).toHaveAttribute("height", "38");
  });

  it("forwards className to the lockup element", () => {
    const { container } = render(<Lockup className="custom-class" />);
    expect(container.firstElementChild).toHaveClass("custom-class");
  });
});
