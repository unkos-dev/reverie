import { afterEach, describe, expect, test } from "vite-plus/test";

import {
  RAIL_STORAGE_KEY,
  readRailCollapsed,
  writeRailCollapsed,
} from "@/pages/library/rail-storage";

afterEach(() => {
  localStorage.clear();
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

test("a full native storage quota does not escape the rail write", () => {
  localStorage.setItem("quota", "x".repeat(5 * 1024 * 1024 - "quota".length));
  expect(() => {
    localStorage.setItem(RAIL_STORAGE_KEY, "true");
  }).toThrow();
  expect(() => {
    writeRailCollapsed(true);
  }).not.toThrow();
  expect(readRailCollapsed()).toBeNull();
});
