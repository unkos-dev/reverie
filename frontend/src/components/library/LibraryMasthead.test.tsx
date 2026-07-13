import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vite-plus/test";

import { LibraryMasthead } from "./LibraryMasthead";

const themeState = { effective: "dark" };

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({
    preference: "system",
    effective: themeState.effective,
    setPreference: vi.fn(),
  }),
}));

describe("LibraryMasthead", () => {
  test("renders the title inside the hero band as the page h1", () => {
    const { container } = render(<LibraryMasthead />);
    const h1 = screen.getByRole("heading", { level: 1, name: "Library" });
    expect(h1).toHaveClass("lib-hero-title");
    expect(container.querySelector(".lib-hero")).toContainElement(h1);
  });

  test("hero photo is decorative (empty alt) and served webp-first", () => {
    const { container } = render(<LibraryMasthead />);
    expect(container.querySelector("img")).toHaveAttribute("alt", "");
    expect(container.querySelector("source")).toHaveAttribute("type", "image/webp");
  });

  test("dark theme renders the dark hero asset", () => {
    themeState.effective = "dark";
    const { container } = render(<LibraryMasthead />);
    expect(container.querySelector("img")).toHaveAttribute(
      "src",
      "/editorial/library-hero-gilt-spines.jpg",
    );
  });

  test("light theme renders the light hero asset", () => {
    themeState.effective = "light";
    const { container } = render(<LibraryMasthead />);
    expect(container.querySelector("img")).toHaveAttribute(
      "src",
      "/editorial/library-hero-gilt-spines-light.jpg",
    );
    themeState.effective = "dark";
  });

  test("renders no fabricated count prose", () => {
    const { container } = render(<LibraryMasthead />);
    expect(container.textContent).not.toMatch(/\d+\s+(volumes|authors|books)/i);
  });

  test("title is overridable", () => {
    render(<LibraryMasthead title="Shelves" />);
    expect(screen.getByRole("heading", { level: 1, name: "Shelves" })).toBeInTheDocument();
  });

  test("hero bleed classes are overridable", () => {
    const { container } = render(<LibraryMasthead heroBleedClassName="mx-0" />);
    expect(container.querySelector(".lib-hero")).toHaveClass("mx-0");
  });
});
