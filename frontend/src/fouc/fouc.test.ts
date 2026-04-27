// Behavioral guard for the inline FOUC script body. The csp-hash plugin
// covers the build-time injection + sha256, but nothing else verifies what
// the script *does* in the browser. Past regressions on this file (notably
// d29a7cc, the </script literal that broke cold-load) escaped detection
// because there was no jsdom evaluation of the script body. These tests
// close that gap.

import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { THEME_COOKIE_NAME } from "../lib/theme/cookie";

const HERE = dirname(fileURLToPath(import.meta.url));
const FOUC_BODY = readFileSync(resolve(HERE, "./fouc.js"), "utf8");

function runFouc(): void {
  // The script is an IIFE; evaluating it in the current jsdom realm via
  // `new Function` is identical to how the browser would run it inline,
  // and it can read/write `document` and `window` from this scope.
  // eslint-disable-next-line @typescript-eslint/no-implied-eval, no-new-func
  new Function(FOUC_BODY)();
}

beforeEach(() => {
  document.cookie = `${THEME_COOKIE_NAME}=; Path=/; Max-Age=0`;
  document.documentElement.removeAttribute("data-theme");
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  document.cookie = `${THEME_COOKIE_NAME}=; Path=/; Max-Age=0`;
  document.documentElement.removeAttribute("data-theme");
});

function stubMatchMedia(prefersDark: boolean): void {
  vi.stubGlobal("matchMedia", () => ({
    matches: prefersDark,
    media: "(prefers-color-scheme: dark)",
    addEventListener: (): void => undefined,
    removeEventListener: (): void => undefined,
    addListener: (): void => undefined,
    removeListener: (): void => undefined,
    dispatchEvent: (): boolean => true,
    onchange: null,
  }));
}

describe("fouc.js", () => {
  test("cookie=dark → data-theme=dark regardless of system preference", () => {
    document.cookie = `${THEME_COOKIE_NAME}=dark`;
    stubMatchMedia(false);

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  test("cookie=light → data-theme=light regardless of system preference", () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    stubMatchMedia(true);

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("light");
  });

  test("cookie=system + system prefers dark → data-theme=dark", () => {
    document.cookie = `${THEME_COOKIE_NAME}=system`;
    stubMatchMedia(true);

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  test("cookie=system + system prefers light → data-theme=light", () => {
    document.cookie = `${THEME_COOKIE_NAME}=system`;
    stubMatchMedia(false);

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("light");
  });

  test("no cookie → falls through to system resolution path", () => {
    stubMatchMedia(true);

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  test("malformed cookie value → falls through to system resolution path", () => {
    document.cookie = `${THEME_COOKIE_NAME}=javascript:alert(1)`;
    stubMatchMedia(false);

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("light");
  });

  test("matchMedia throws → catch fallback sets light + warns", () => {
    document.cookie = `${THEME_COOKIE_NAME}=system`;
    vi.stubGlobal("matchMedia", () => {
      throw new Error("matchMedia unavailable");
    });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("light");
    expect(warn).toHaveBeenCalled();
    const [msg] = warn.mock.calls[0] ?? [];
    expect(typeof msg).toBe("string");
    expect(msg).toMatch(/FOUC/);
  });
});
