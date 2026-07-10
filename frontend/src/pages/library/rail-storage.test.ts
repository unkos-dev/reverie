import { afterEach, describe, expect, test, vi } from "vite-plus/test";

import { RAIL_STORAGE_KEY, readRailCollapsed, writeRailCollapsed } from "./rail-storage";

afterEach(() => {
  localStorage.removeItem(RAIL_STORAGE_KEY);
  vi.restoreAllMocks();
});

describe("writeRailCollapsed / readRailCollapsed round-trip", () => {
  test("round-trips true", () => {
    writeRailCollapsed(true);
    expect(readRailCollapsed()).toBe(true);
  });

  test("round-trips false", () => {
    writeRailCollapsed(false);
    expect(readRailCollapsed()).toBe(false);
  });
});

describe("readRailCollapsed edge cases", () => {
  test("returns null when unset", () => {
    expect(readRailCollapsed()).toBeNull();
  });

  test("returns null for a garbage stored value", () => {
    localStorage.setItem(RAIL_STORAGE_KEY, "collapsed");
    expect(readRailCollapsed()).toBeNull();
  });

  test("returns null when localStorage.getItem throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("storage disabled");
    });
    expect(readRailCollapsed()).toBeNull();
  });
});

describe("writeRailCollapsed failure handling", () => {
  test("does not throw when localStorage.setItem throws", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("quota exceeded");
    });
    expect(() => {
      writeRailCollapsed(true);
    }).not.toThrow();
  });
});
