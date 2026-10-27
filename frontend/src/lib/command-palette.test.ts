import { afterEach, describe, expect, test, vi } from "vite-plus/test";

import { openCommandPalette, searchHintLabel, setCommandPaletteOpener } from "./command-palette";

afterEach(() => {
  setCommandPaletteOpener(() => {});
  vi.restoreAllMocks();
});

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
    expect(searchHintLabel({ platform: "MacIntel" })).toBe("⌘K");
  });

  test("returns Ctrl K elsewhere", () => {
    expect(searchHintLabel({ platform: "Win32" })).toBe("Ctrl K");
  });

  test("prefers userAgentData.platform over navigator.platform", () => {
    expect(searchHintLabel({ platform: "Win32", userAgentData: { platform: "macOS" } })).toBe("⌘K");
  });

  test("falls back to navigator.platform when userAgentData carries no platform", () => {
    expect(searchHintLabel({ platform: "MacIntel", userAgentData: {} })).toBe("⌘K");
  });
});
