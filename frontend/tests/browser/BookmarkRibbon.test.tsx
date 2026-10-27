import { render } from "vitest-browser-react";
import { describe, expect, test } from "vite-plus/test";

import { BookmarkRibbon } from "@/components/library/BookmarkRibbon";

describe("BookmarkRibbon", () => {
  test("renders a decorative, chrome-marked ribbon", async () => {
    const { container } = await render(<BookmarkRibbon />);
    const ribbon = container.querySelector<HTMLElement>("[data-chrome]");
    await expect.element(ribbon).toBeInTheDocument();

    await expect.element(ribbon).toHaveAttribute("aria-hidden", "true");

    await expect.element(ribbon?.querySelector<HTMLElement>("[style]") ?? null).toBeInTheDocument();
  });
});
