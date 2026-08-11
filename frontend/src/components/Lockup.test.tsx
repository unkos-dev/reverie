import { describe, it, expect } from "vite-plus/test";
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

  it("uses the canonical thick-slot asset below 24px", () => {
    const { container } = render(<Lockup size={13} />);
    expect(container.querySelector("img")).toHaveAttribute("src", "/brand/glyph/slot-favicon.svg");
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

  it("sizes the glyph and wordmark gap from cap height", () => {
    const { container } = render(<Lockup size={40} />);
    expect(container.firstElementChild).toHaveStyle({ fontSize: "40px", gap: "0.5cap" });
    expect(container.querySelector("img")).toHaveAttribute(
      "style",
      expect.stringContaining("width: 0.95cap"),
    );
    expect(container.querySelector("img")).toHaveAttribute(
      "style",
      expect.stringContaining("height: 0.95cap"),
    );
  });

  it("forwards className to the lockup element", () => {
    const { container } = render(<Lockup className="custom-class" />);
    expect(container.firstElementChild).toHaveClass("custom-class");
  });
});
