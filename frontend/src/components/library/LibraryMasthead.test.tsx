import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vite-plus/test";

import { LibraryMasthead } from "./LibraryMasthead";

describe("LibraryMasthead", () => {
  test("renders the gilt title as the page h1", () => {
    render(<LibraryMasthead />);
    const h1 = screen.getByRole("heading", { level: 1, name: "Library" });
    expect(h1).toBeInTheDocument();
    expect(h1).toHaveClass("lib-h1-gilt");
  });

  test("renders the curatorial kicker", () => {
    render(<LibraryMasthead />);
    expect(screen.getByText("Your library, catalogued.")).toBeInTheDocument();
  });

  test("hero photo is decorative (empty alt) and served webp-first", () => {
    const { container } = render(<LibraryMasthead />);
    expect(container.querySelector("img")).toHaveAttribute("alt", "");
    expect(container.querySelector("source")).toHaveAttribute("type", "image/webp");
  });

  test("title and kicker are overridable", () => {
    render(<LibraryMasthead title="Shelves" kicker="A test kicker" />);
    expect(screen.getByRole("heading", { level: 1, name: "Shelves" })).toBeInTheDocument();
    expect(screen.getByText("A test kicker")).toBeInTheDocument();
  });

  test("hero bleed classes are overridable", () => {
    const { container } = render(<LibraryMasthead heroBleedClassName="mx-0" />);
    expect(container.querySelector(".lib-hero")).toHaveClass("mx-0");
  });
});
