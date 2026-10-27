import type { ReactElement } from "react";
import { afterEach, describe, expect, test } from "vite-plus/test";
import { page, userEvent } from "vite-plus/test/browser";
import { render } from "vitest-browser-react";

import { useCinematicMode } from "@/hooks/useCinematicMode";

function Probe(): ReactElement {
  const active = useCinematicMode();
  return (
    <>
      <output aria-label="Cinematic mode">{String(active)}</output>
      <input aria-label="Search" />
    </>
  );
}

afterEach(() => {
  document.documentElement.removeAttribute("data-cinematic");
  document.documentElement.removeAttribute("data-cursor-hidden");
});

describe("useCinematicMode", () => {
  test("F toggles cinematic mode and reflects it on the document element", async () => {
    await render(<Probe />);
    await expect.element(page.getByLabelText("Cinematic mode")).toHaveTextContent("false");
    await userEvent.keyboard("f");
    await expect.element(page.getByLabelText("Cinematic mode")).toHaveTextContent("true");
    expect(document.documentElement.getAttribute("data-cinematic")).toBe("on");
    await userEvent.keyboard("f");
    await expect.element(page.getByLabelText("Cinematic mode")).toHaveTextContent("false");
    expect(document.documentElement.hasAttribute("data-cinematic")).toBe(false);
  });

  test("Escape exits cinematic mode", async () => {
    await render(<Probe />);
    await userEvent.keyboard("f");
    await expect.element(page.getByLabelText("Cinematic mode")).toHaveTextContent("true");
    await userEvent.keyboard("{Escape}");
    await expect.element(page.getByLabelText("Cinematic mode")).toHaveTextContent("false");
  });

  test("Escape is ignored while typing in a field", async () => {
    await render(<Probe />);
    await userEvent.keyboard("f");
    await expect.element(page.getByLabelText("Cinematic mode")).toHaveTextContent("true");
    await userEvent.click(page.getByRole("textbox", { name: "Search" }));
    await userEvent.keyboard("{Escape}");
    await expect.element(page.getByLabelText("Cinematic mode")).toHaveTextContent("true");
  });

  test("F is ignored while typing in a field", async () => {
    await render(<Probe />);
    await userEvent.click(page.getByRole("textbox", { name: "Search" }));
    await userEvent.keyboard("f");
    await expect.element(page.getByLabelText("Cinematic mode")).toHaveTextContent("false");
    await expect.element(page.getByRole("textbox", { name: "Search" })).toHaveValue("f");
  });
});
