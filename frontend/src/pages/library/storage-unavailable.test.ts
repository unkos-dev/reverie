import { expect, test } from "vite-plus/test";

import { activeUserId, forgetActiveUser, rememberActiveUser } from "@/lib/active-user";

import { readDisplayPreferences, writeDisplayPreferences } from "./display-storage";
import { readRailCollapsed, writeRailCollapsed } from "./rail-storage";

test("unavailable storage preserves display and rail fallback contracts", () => {
  expect(() => localStorage.getItem("reverie_library_rail")).toThrow();
  rememberActiveUser("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
  expect(readDisplayPreferences()).toEqual({
    density: null,
    hiddenColumns: null,
    view: null,
    sortStack: null,
  });
  expect(() => {
    writeDisplayPreferences({ density: "compact" });
  }).not.toThrow();
  expect(readRailCollapsed()).toBeNull();
  expect(() => {
    writeRailCollapsed(true);
  }).not.toThrow();
  forgetActiveUser();
});

test("unavailable storage does not escape active-account operations", () => {
  expect(activeUserId()).toBeNull();
  expect(() => {
    rememberActiveUser("user-a");
  }).not.toThrow();
  expect(() => {
    forgetActiveUser();
  }).not.toThrow();
});
