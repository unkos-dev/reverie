import { render } from "vitest-browser-react";
import { describe, expect, test } from "vite-plus/test";

import { Atmosphere } from "@/components/library/Atmosphere";

describe("Atmosphere", () => {
  test("renders the ember field and grain as decorative layers", async () => {
    const { container } = await render(<Atmosphere />);
    const atm = container.querySelector<HTMLElement>("@/components/library/.lib-atm");
    const grain = container.querySelector<HTMLElement>("@/components/library/.lib-grain");
    await expect.element(atm).toBeInTheDocument();
    await expect.element(grain).toBeInTheDocument();
    await expect.element(atm).toHaveAttribute("aria-hidden", "true");
    await expect.element(grain).toHaveAttribute("aria-hidden", "true");
  });
});
