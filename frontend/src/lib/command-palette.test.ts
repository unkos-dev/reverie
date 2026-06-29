import { afterEach, describe, expect, test, vi } from "vite-plus/test";

import { openCommandPalette, searchHintLabel, setCommandPaletteOpener } from "./command-palette";

afterEach(() => {
  setCommandPaletteOpener(() => {});
  vi.restoreAllMocks();
});

function mockPlatform(platform: string): void {
  Object.defineProperty(window.navigator, "platform", {
    configurable: true,
    get: () => platform,
  });
}

function mockUserAgentData(value: { platform?: string } | undefined): void {
  Object.defineProperty(window.navigator, "userAgentData", {
    configurable: true,
    value,
  });
}

describe("openCommandPalette", () => {
  test("invokes the registered opener", () => {
    const opener = vi.fn();
    setCommandPaletteOpener(opener);
    openCommandPalette();
    expect(opener).toHaveBeenCalledTimes(1);
  });

  test("is a no-op when no opener is registered", () => {
    expect(() => {
      openCommandPalette();
    }).not.toThrow();
  });
});

describe("searchHintLabel", () => {
  test("returns ⌘K on mac platforms", () => {
    mockPlatform("MacIntel");
    expect(searchHintLabel()).toBe("⌘K");
  });

  test("returns Ctrl K elsewhere", () => {
    mockPlatform("Win32");
    expect(searchHintLabel()).toBe("Ctrl K");
  });

  test("prefers userAgentData.platform over navigator.platform", () => {
    mockUserAgentData({ platform: "macOS" });
    mockPlatform("Win32");
    expect(searchHintLabel()).toBe("⌘K");
    mockUserAgentData(undefined);
  });

  test("falls back to navigator.platform when userAgentData carries no platform", () => {
    mockUserAgentData({});
    mockPlatform("MacIntel");
    expect(searchHintLabel()).toBe("⌘K");
    mockUserAgentData(undefined);
  });
});
