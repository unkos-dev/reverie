import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import FOUC_BODY from "@/fouc/fouc.js?raw";
import { cdp } from "vite-plus/test/browser";
import { THEME_COOKIE_NAME } from "@/lib/theme/cookie";

function runFouc(): void {
  const script = document.createElement("script");
  script.textContent = FOUC_BODY;
  document.head.append(script);
  script.remove();
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

describe("fouc.js", () => {
  test("cookie=dark → data-theme=dark regardless of system preference", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=dark`;
    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "light" }],
    });

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  test("cookie=light → data-theme=light regardless of system preference", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "dark" }],
    });

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("light");
  });

  test("cookie=system + system prefers dark → data-theme=dark", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=system`;
    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "dark" }],
    });

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  test("cookie=system + system prefers light → data-theme=light", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=system`;
    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "light" }],
    });

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("light");
  });

  test("no cookie → falls through to system resolution path", async () => {
    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "dark" }],
    });

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  test("malformed cookie value → falls through to system resolution path", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=javascript:alert(1)`;
    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "light" }],
    });

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
    const msg: unknown = warn.mock.calls[0]?.[0];
    expect(typeof msg).toBe("string");
    expect(msg).toMatch(/FOUC/);
  });
});
