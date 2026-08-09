import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { forgetActiveUser, rememberActiveUser } from "@/lib/active-user";

import {
  displayStorageKey,
  readDisplayPreferences,
  writeDisplayPreferences,
} from "./display-storage";

const NO_HINT = { density: null, hiddenColumns: null, view: null, sortStack: null };

const USER_A = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const USER_B = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";

beforeEach(() => {
  rememberActiveUser(USER_A);
});

afterEach(() => {
  localStorage.clear();
});

describe("display-storage", () => {
  test("round-trips every mirrored group under the active account's key", () => {
    writeDisplayPreferences({
      density: "compact",
      hiddenColumns: ["isbn_13", "pages"],
      view: "table",
      sortStack: "title,-pages",
    });
    expect(readDisplayPreferences()).toEqual({
      density: "compact",
      hiddenColumns: ["isbn_13", "pages"],
      view: "table",
      sortStack: "title,-pages",
    });
    expect(localStorage.getItem(displayStorageKey(USER_A))).not.toBeNull();
  });

  test("absent storage reads as no hint", () => {
    expect(readDisplayPreferences()).toEqual(NO_HINT);
  });

  test("with no confirmed account, reads are empty and writes are dropped", () => {
    forgetActiveUser();
    writeDisplayPreferences({ density: "compact" });
    expect(readDisplayPreferences()).toEqual(NO_HINT);
    expect(localStorage.length).toBe(0);
  });

  test("one account's mirror never reads as another's", () => {
    writeDisplayPreferences({ density: "compact", sortStack: "-pages" });
    rememberActiveUser(USER_B);
    expect(readDisplayPreferences()).toEqual(NO_HINT);
    // A's mirror survives untouched for A's return.
    rememberActiveUser(USER_A);
    expect(readDisplayPreferences().density).toBe("compact");
  });

  test("sign-out keeps the mirror; the returning account gets it back", () => {
    writeDisplayPreferences({ view: "table" });
    forgetActiveUser();
    expect(readDisplayPreferences()).toEqual(NO_HINT);
    rememberActiveUser(USER_A);
    expect(readDisplayPreferences().view).toBe("table");
  });

  test("a legacy device-global entry is deleted on read, never consulted", () => {
    localStorage.setItem(
      "reverie_library_display",
      JSON.stringify({ density: "compact", hiddenColumns: ["subtitle"] }),
    );
    expect(readDisplayPreferences()).toEqual(NO_HINT);
    expect(localStorage.getItem("reverie_library_display")).toBeNull();
  });

  test("a density write preserves the other mirrored groups", () => {
    writeDisplayPreferences({ hiddenColumns: ["rating"], view: "table" });
    writeDisplayPreferences({ density: "comfortable" });
    expect(readDisplayPreferences()).toEqual({
      density: "comfortable",
      hiddenColumns: ["rating"],
      view: "table",
      sortStack: null,
    });
  });

  test("malformed JSON reads as no hint", () => {
    localStorage.setItem(displayStorageKey(USER_A), "{not json");
    expect(readDisplayPreferences()).toEqual(NO_HINT);
  });

  test("unknown density value degrades to null without dropping columns", () => {
    localStorage.setItem(
      displayStorageKey(USER_A),
      JSON.stringify({ density: "roomy", hiddenColumns: ["series"] }),
    );
    expect(readDisplayPreferences()).toEqual({ ...NO_HINT, hiddenColumns: ["series"] });
  });

  test("unknown view value degrades to null without dropping density", () => {
    localStorage.setItem(
      displayStorageKey(USER_A),
      JSON.stringify({ density: "compact", view: "list" }),
    );
    expect(readDisplayPreferences()).toEqual({ ...NO_HINT, density: "compact" });
  });

  test("non-string entries in hiddenColumns degrade the field to null", () => {
    localStorage.setItem(
      displayStorageKey(USER_A),
      JSON.stringify({ density: "compact", hiddenColumns: [3, "series"] }),
    );
    expect(readDisplayPreferences()).toEqual({ ...NO_HINT, density: "compact" });
  });

  test("a payload missing newer groups still supplies the ones it carries", () => {
    localStorage.setItem(
      displayStorageKey(USER_A),
      JSON.stringify({ density: "compact", hiddenColumns: ["series"] }),
    );
    expect(readDisplayPreferences()).toEqual({
      density: "compact",
      hiddenColumns: ["series"],
      view: null,
      sortStack: null,
    });
  });

  test("throwing storage degrades silently on read and write", () => {
    const spy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(readDisplayPreferences()).toEqual(NO_HINT);
    expect(() => {
      writeDisplayPreferences({ density: "compact" });
    }).not.toThrow();
    spy.mockRestore();
  });
});
