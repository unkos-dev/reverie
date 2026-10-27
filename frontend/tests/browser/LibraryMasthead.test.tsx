import { render } from "vitest-browser-react";
import { page } from "vite-plus/test/browser";
import { describe, expect, test, vi } from "vite-plus/test";

import { LibraryMasthead } from "@/components/library/LibraryMasthead";

const themeState = { effective: "dark" };

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({
    preference: "system",
    effective: themeState.effective,
    setPreference: vi.fn(),
  }),
}));

describe("LibraryMasthead", () => {
  test("renders the title inside the hero band as the page h1", async () => {
    const { container } = await render(<LibraryMasthead />);
    const h1 = page.getByRole("heading", { level: 1, name: "Library" });
    await expect.element(h1).toHaveClass("lib-hero-title");
    await expect
      .element(container.querySelector<HTMLElement>(".lib-hero"))
      .toContainElement(h1.element());
  });

  test("hero photo is decorative (empty alt) and served webp-first", async () => {
    const { container } = await render(<LibraryMasthead />);
    await expect.element(container.querySelector<HTMLElement>("img")).toHaveAttribute("alt", "");
    await expect
      .element(container.querySelector<HTMLElement>("source"))
      .toHaveAttribute("type", "image/webp");
  });

  test("dark theme renders the dark hero asset", async () => {
    themeState.effective = "dark";
    const { container } = await render(<LibraryMasthead />);
    await expect
      .element(container.querySelector<HTMLElement>("img"))
      .toHaveAttribute("src", "/editorial/library-hero-gilt-spines.jpg");
  });

  test("light theme renders the light hero asset", async () => {
    themeState.effective = "light";
    const { container } = await render(<LibraryMasthead />);
    await expect
      .element(container.querySelector<HTMLElement>("img"))
      .toHaveAttribute("src", "/editorial/library-hero-gilt-spines-light.jpg");
    themeState.effective = "dark";
  });

  test("renders no fabricated count prose", async () => {
    const { container } = await render(<LibraryMasthead />);
    expect(container.textContent).not.toMatch(/\d+\s+(volumes|authors|books)/i);
  });

  test("title is overridable", async () => {
    await render(<LibraryMasthead title="Shelves" />);
    await expect
      .element(page.getByRole("heading", { level: 1, name: "Shelves" }))
      .toBeInTheDocument();
  });

  test("hero bleed classes are overridable", async () => {
    const { container } = await render(<LibraryMasthead heroBleedClassName="mx-0" />);
    await expect.element(container.querySelector<HTMLElement>(".lib-hero")).toHaveClass("mx-0");
  });
});
