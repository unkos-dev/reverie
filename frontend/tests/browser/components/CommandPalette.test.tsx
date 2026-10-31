import { render } from "vitest-browser-react";
import { expect, test } from "vite-plus/test";

import { HighlightedSnippet } from "@/components/CommandPalette";

test("HTML in the snippet renders as literal text, not as markup", async () => {
  const { container } = await render(
    <HighlightedSnippet text={"\u0002<script>alert(1)</script>\u0003"} />,
  );
  expect(container.querySelector("script")).toBeNull();
  expect(document.querySelector("mark")?.textContent).toBe("<script>alert(1)</script>");
});
