import { render } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { Atmosphere } from "./Atmosphere";

describe("Atmosphere", () => {
  test("renders the ember field and grain as decorative layers", () => {
    const { container } = render(<Atmosphere />);
    const atm = container.querySelector(".lib-atm");
    const grain = container.querySelector(".lib-grain");
    expect(atm).toBeInTheDocument();
    expect(grain).toBeInTheDocument();
    expect(atm).toHaveAttribute("aria-hidden", "true");
    expect(grain).toHaveAttribute("aria-hidden", "true");
  });
});
