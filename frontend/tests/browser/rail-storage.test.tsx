import { afterEach, describe, expect, test } from "vite-plus/test";

import {
  RAIL_STORAGE_KEY,
  readRailCollapsed,
  writeRailCollapsed,
} from "@/pages/library/rail-storage";

afterEach(() => {
  localStorage.removeItem(RAIL_STORAGE_KEY);
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
});

describe("writeRailCollapsed failure handling", () => {});
