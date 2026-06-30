import { render } from "@testing-library/react";
import { describe, expect, test } from "vite-plus/test";

import { BookmarkRibbon } from "./BookmarkRibbon";

describe("BookmarkRibbon", () => {
  test("renders a decorative, chrome-marked ribbon", () => {
    const { container } = render(<BookmarkRibbon />);
    const ribbon = container.querySelector("[data-chrome]");
    expect(ribbon).toBeInTheDocument();
    // Decorative — the native scrollbar carries the real semantics.
    expect(ribbon).toHaveAttribute("aria-hidden", "true");
    // The thumb carries a positioning style (scroll progress).
    expect(ribbon?.querySelector("[style]")).toBeInTheDocument();
  });
});
