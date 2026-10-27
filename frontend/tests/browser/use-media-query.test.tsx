import type { ReactElement } from "react";
import { afterEach, expect, test, vi } from "vite-plus/test";
import { page } from "vite-plus/test/browser";
import { render } from "vitest-browser-react";

import { useMediaQuery } from "@/lib/hooks/use-media-query";

function Probe(): ReactElement {
  const matches = useMediaQuery("(min-width: 1024px)");
  return <output aria-label="Wide viewport">{String(matches)}</output>;
}

afterEach(async () => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  await page.viewport(1280, 800);
});

test("reflects the initial match state", async () => {
  await page.viewport(1280, 800);
  await render(<Probe />);
  await expect.element(page.getByLabelText("Wide viewport")).toHaveTextContent("true");
});

test("updates when the native media query changes", async () => {
  await page.viewport(800, 800);
  await render(<Probe />);
  await expect.element(page.getByLabelText("Wide viewport")).toHaveTextContent("false");
  await page.viewport(1280, 800);
  await expect.element(page.getByLabelText("Wide viewport")).toHaveTextContent("true");
});

test("unsubscribes the registered listener on unmount", async () => {
  const add = vi.spyOn(MediaQueryList.prototype, "addEventListener");
  const remove = vi.spyOn(MediaQueryList.prototype, "removeEventListener");
  const { unmount } = await render(<Probe />);
  const subscription = add.mock.calls.find(([event]) => event === "change");
  expect(subscription).toBeDefined();
  await unmount();
  expect(remove).toHaveBeenCalledWith("change", subscription?.[1]);
});

test("returns false when matchMedia is unavailable", async () => {
  vi.stubGlobal("matchMedia", undefined);
  await render(<Probe />);
  await expect.element(page.getByLabelText("Wide viewport")).toHaveTextContent("false");
});
